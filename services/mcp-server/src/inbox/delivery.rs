//! Atomic keyed delivery: receipt, recipient message, sent copy and accounting
//! commit together. A retry cannot send again, even after a lost HTTP response.
use super::*;
use backoff::Error::Permanent;
use sha2::{Digest, Sha256};

const COLLECTION: &str = "inbox_send_deliveries";

#[derive(Clone, Serialize, Deserialize)]
struct Delivery {
    fingerprint: String,
    receipt: SendReceipt,
}

pub(super) fn validate_key(key: &str) -> Result<(), InboxRejection> {
    if key.is_empty()
        || key.len() > 128
        || !key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err(InboxRejection::InvalidRequest(
            "delivery_key must contain 1–128 ASCII letters, digits, underscores or hyphens".into(),
        ));
    }
    Ok(())
}

fn digest(parts: &[&str]) -> String {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update((part.len() as u64).to_be_bytes());
        hash.update(part.as_bytes());
    }
    hex::encode(hash.finalize())
}

type TransactionError = backoff::Error<firestore::errors::FirestoreError>;
type DeliveryOutcome = Result<(SendReceipt, bool), InboxRejection>;

#[derive(Clone)]
struct ProposedDelivery {
    id: String,
    fingerprint: String,
    message: InboxMessageDoc,
    limit: u32,
    quota_expiry: FirestoreTimestamp,
}

struct Accounting {
    quota_id: String,
    quota: QuotaDoc,
    thread: ThreadMetaDoc,
    mailbox: MailboxMetaDoc,
}

impl Accounting {
    async fn load(db: &FirestoreDb, proposed: &ProposedDelivery) -> Result<Self, TransactionError> {
        let doc = &proposed.message;
        let parent = db
            .parent_path(MAILBOXES_COLLECTION, &doc.to_wallet)
            .map_err(Permanent)?;
        let day = quota_day(doc.sent_at.0);
        let quota_id = format!("{}:{day}", doc.from_wallet);
        let quota: Option<QuotaDoc> = db
            .fluent()
            .select()
            .by_id_in(INBOX_QUOTAS_COLLECTION)
            .obj()
            .one(&quota_id)
            .await
            .map_err(Permanent)?;
        let thread: Option<ThreadMetaDoc> = db
            .fluent()
            .select()
            .by_id_in(INBOX_THREADS_SUBCOLLECTION)
            .parent(&parent)
            .obj()
            .one(&doc.thread_id)
            .await
            .map_err(Permanent)?;
        let mailbox: Option<MailboxMetaDoc> = db
            .fluent()
            .select()
            .by_id_in(MAILBOXES_COLLECTION)
            .obj()
            .one(&doc.to_wallet)
            .await
            .map_err(Permanent)?;
        Ok(Self {
            quota_id,
            quota: quota.unwrap_or(QuotaDoc {
                wallet: doc.from_wallet.clone(),
                date: day,
                sends: 0,
                reads: 0,
                posts: 0,
                expires_at: proposed.quota_expiry.clone(),
            }),
            thread: thread.unwrap_or(ThreadMetaDoc {
                thread_id: doc.thread_id.clone(),
                message_count: 0,
                muted: false,
                reported: false,
                last_msg_at: None,
                expires_at: None,
            }),
            mailbox: mailbox.unwrap_or(MailboxMetaDoc {
                wallet: doc.to_wallet.clone(),
                unread_count: 0,
                latest_cursor: String::new(),
                read_watermark: String::new(),
                updated_at: doc.sent_at.clone(),
            }),
        })
    }

    fn record(&mut self, proposed: &ProposedDelivery) -> Result<SendReceipt, InboxRejection> {
        let doc = &proposed.message;
        if self.quota.sends >= i64::from(proposed.limit) {
            return Err(InboxRejection::SendQuotaExceeded {
                limit: proposed.limit,
            });
        }
        if self.thread.muted {
            return Err(InboxRejection::ThreadMuted);
        }
        if self.thread.message_count >= limits::THREAD_MESSAGE_CAP {
            return Err(InboxRejection::ThreadFull);
        }
        self.quota.sends = self.quota.sends.saturating_add(1);
        self.thread.message_count = self.thread.message_count.saturating_add(1);
        self.thread.last_msg_at = Some(doc.sent_at.clone());
        self.mailbox.unread_count = self.mailbox.unread_count.saturating_add(1);
        if doc.msg_id > self.mailbox.latest_cursor {
            self.mailbox.latest_cursor = doc.msg_id.clone();
        }
        self.mailbox.updated_at = doc.sent_at.clone();
        Ok(SendReceipt {
            msg_id: doc.msg_id.clone(),
            to: doc.to_wallet.clone(),
            thread_id: doc.thread_id.clone(),
            intent: doc.intent.clone(),
            bytes: doc.body.len(),
            expires_at: None,
            sends_remaining_today: u32::try_from(
                i64::from(proposed.limit)
                    .saturating_sub(self.quota.sends)
                    .max(0),
            )
            .unwrap_or(0),
        })
    }

    fn persist(
        &self,
        db: &FirestoreDb,
        tx: &mut firestore::FirestoreTransaction<'_>,
        doc: &InboxMessageDoc,
    ) -> Result<(), TransactionError> {
        let parent = db
            .parent_path(MAILBOXES_COLLECTION, &doc.to_wallet)
            .map_err(Permanent)?;
        db.fluent()
            .update()
            .in_col(INBOX_QUOTAS_COLLECTION)
            .document_id(&self.quota_id)
            .object(&self.quota)
            .add_to_transaction(tx)
            .map_err(Permanent)?;
        db.fluent()
            .update()
            .in_col(INBOX_THREADS_SUBCOLLECTION)
            .document_id(&doc.thread_id)
            .parent(&parent)
            .object(&self.thread)
            .add_to_transaction(tx)
            .map_err(Permanent)?;
        db.fluent()
            .update()
            .in_col(MAILBOXES_COLLECTION)
            .document_id(&doc.to_wallet)
            .object(&self.mailbox)
            .add_to_transaction(tx)
            .map_err(Permanent)?;
        db.fluent()
            .update()
            .in_col(INBOX_MESSAGES_SUBCOLLECTION)
            .document_id(&doc.msg_id)
            .parent(&parent)
            .object(doc)
            .add_to_transaction(tx)
            .map_err(Permanent)?;
        if mailbox_address(&doc.from_wallet).is_ok() {
            let parent = db
                .parent_path(MAILBOXES_COLLECTION, &doc.from_wallet)
                .map_err(Permanent)?;
            let mirror = InboxMessageDoc {
                direction: DIRECTION_SENT.into(),
                ..doc.clone()
            };
            db.fluent()
                .update()
                .in_col(INBOX_SENT_SUBCOLLECTION)
                .document_id(&doc.msg_id)
                .parent(&parent)
                .object(&mirror)
                .add_to_transaction(tx)
                .map_err(Permanent)?;
        }
        Ok(())
    }
}

async fn commit(
    db: FirestoreDb,
    tx: &mut firestore::FirestoreTransaction<'_>,
    proposed: ProposedDelivery,
) -> Result<DeliveryOutcome, TransactionError> {
    let existing: Option<Delivery> = db
        .fluent()
        .select()
        .by_id_in(COLLECTION)
        .obj()
        .one(&proposed.id)
        .await
        .map_err(Permanent)?;
    if let Some(existing) = existing {
        return Ok(if existing.fingerprint == proposed.fingerprint {
            Ok((existing.receipt, false))
        } else {
            Err(InboxRejection::InvalidRequest(
                "delivery_key was already used with different message content".into(),
            ))
        });
    }
    let mut accounting = Accounting::load(&db, &proposed).await?;
    let receipt = match accounting.record(&proposed) {
        Ok(receipt) => receipt,
        Err(error) => return Ok(Err(error)),
    };
    accounting.persist(&db, tx, &proposed.message)?;
    db.fluent()
        .update()
        .in_col(COLLECTION)
        .document_id(&proposed.id)
        .object(&Delivery {
            fingerprint: proposed.fingerprint,
            receipt: receipt.clone(),
        })
        .add_to_transaction(tx)
        .map_err(Permanent)?;
    Ok(Ok((receipt, true)))
}

impl Inbox {
    pub(super) async fn send_keyed(
        &self,
        req: SendRequest,
        to: String,
        thread_id: String,
        intent: Option<String>,
        key: &str,
    ) -> Result<SendReceipt, InboxError> {
        let now = chrono::Utc::now();
        let quota_expiry = now
            .checked_add_signed(chrono::Duration::days(limits::QUOTA_TTL_DAYS))
            .context("quota expiry overflow")?;
        let proposed = ProposedDelivery {
            id: digest(&["inbox-delivery-v1", &req.from, key]),
            fingerprint: digest(&[&to, &thread_id, intent.as_deref().unwrap_or(""), &req.body]),
            limit: effective_send_limit(req.tier, is_support_mailbox(&to)),
            quota_expiry: FirestoreTimestamp(quota_expiry),
            message: InboxMessageDoc {
                schema: MESSAGE_SCHEMA.into(),
                msg_id: new_msg_id(now),
                from_wallet: req.from.clone(),
                to_wallet: to.clone(),
                thread_id: thread_id.clone(),
                intent,
                body: req.body.clone(),
                sent_at: FirestoreTimestamp(now),
                seed: req.seed,
                direction: DIRECTION_RECEIVED.into(),
            },
        };
        let (receipt, fresh) = self
            .db
            .run_transaction_with_options(
                |db, tx| Box::pin(commit(db, tx, proposed.clone())),
                firestore::FirestoreTransactionOptions::new()
                    .with_max_elapsed_time(chrono::Duration::seconds(15)),
            )
            .await
            .context("commit keyed inbox delivery")??;
        if fresh {
            self.notify_recipient_webhook(&to, &req.from, &thread_id, &receipt.msg_id, now)
                .await;
            self.notify_support_responder(&to, &req.from, &thread_id, &receipt.msg_id, &req.body)
                .await;
        }
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keys_are_bounded_and_scoped_without_tuple_ambiguity() {
        for key in ["", "has/slash", "é"] {
            assert!(validate_key(key).is_err());
        }
        assert!(validate_key(&"a".repeat(129)).is_err());
        assert!(validate_key("support_123-abc").is_ok());
        assert_ne!(digest(&["ab", "c"]), digest(&["a", "bc"]));
        assert_ne!(digest(&["alice", "same"]), digest(&["bob", "same"]));
    }
    #[tokio::test]
    #[ignore = "dedicated isolated Firestore database"]
    async fn keyed_delivery_is_atomic_and_sender_scoped() {
        use solana_sdk::signer::Signer;
        let project = std::env::var("SETTLEMENT_TEST_PROJECT").expect("explicit project");
        let database = std::env::var("SETTLEMENT_TEST_DATABASE").expect("explicit database");
        assert_eq!(database, "combined-candidate-verification");
        let db = Arc::new(
            FirestoreDb::with_options(
                firestore::FirestoreDbOptions::new(project).with_database_id(database),
            )
            .await
            .unwrap(),
        );
        let wallet = || {
            mailbox_address(&solana_sdk::signature::Keypair::new().pubkey().to_string()).unwrap()
        };
        let sender = wallet();
        let recipient = wallet();
        let inbox = Inbox::new(db.clone(), None, String::new(), String::new(), None);
        let request = |body: &str| SendRequest {
            from: sender.clone(),
            to_wallet: recipient.clone(),
            body: body.into(),
            thread_id: Some("delivery-check".into()),
            intent: None,
            tier: SenderTier::WalletVerified,
            seed: true,
        };
        let (a, b) = tokio::join!(
            inbox.send_message_with_key(request("synthetic"), Some("retry")),
            inbox.send_message_with_key(request("synthetic"), Some("retry"))
        );
        let a = a.unwrap();
        assert_eq!(a.msg_id, b.unwrap().msg_id);
        assert_eq!(
            a.msg_id,
            inbox
                .send_message_with_key(request("synthetic"), Some("retry"))
                .await
                .unwrap()
                .msg_id
        );
        assert!(inbox
            .send_message_with_key(request("changed"), Some("retry"))
            .await
            .is_err());
        let quota = inbox
            .read_quota(&sender, &quota_day(chrono::Utc::now()))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(quota.sends, 1);
        for (owner, collection) in [
            (&recipient, INBOX_MESSAGES_SUBCOLLECTION),
            (&sender, INBOX_SENT_SUBCOLLECTION),
        ] {
            let rows: Vec<InboxMessageDoc> = db
                .fluent()
                .select()
                .from(collection)
                .parent(inbox.mailbox_parent(owner).unwrap())
                .obj()
                .query()
                .await
                .unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].msg_id, a.msg_id);
        }
        let mut other = request("synthetic");
        other.from = wallet();
        assert_ne!(
            a.msg_id,
            inbox
                .send_message_with_key(other, Some("retry"))
                .await
                .unwrap()
                .msg_id
        );
    }
}

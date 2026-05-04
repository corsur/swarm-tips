#[derive(Debug, thiserror::Error)]
pub enum ScorerError {
    #[error("arithmetic overflow in scoring computation")]
    ArithmeticOverflow,

    #[error("invalid weight configuration: {0}")]
    InvalidWeights(String),

    #[error("invalid scale factor: {0}")]
    InvalidScaleFactor(String),
}

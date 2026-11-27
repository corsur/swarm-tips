/-
Copyright 2026 Swarm Tips contributors
SPDX-License-Identifier: Apache-2.0
-/

-- Intentionally does not import `SwarmProofs.Generated.*`.
-- Content-addressed proof modules must be imported explicitly so a deprecated
-- or revoked result never enters a later task by accident.
import SwarmProofs.Registry
import SwarmProofs.Chains.BoolFourier.Defs.V1
import SwarmProofs.Chains.BSD.Defs.V1
import SwarmProofs.Chains.Circuits.Defs.V1
import SwarmProofs.Chains.NavierStokes.Defs.V1
import SwarmProofs.Chains.ProtocolMath.Defs.V1
import SwarmProofs.Chains.QuantumInfo.Defs.V1
import SwarmProofs.Chains.RHAnalytic.Defs.V1
import SwarmProofs.Chains.RHLagarias.Defs.V1

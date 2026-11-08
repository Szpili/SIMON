//! I4 (M7) — Information Flow Control (IFC). Deny-by-default on sinks.

use serde::{Deserialize, Serialize};

/// Origin and trust label of value/content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    Operator,
    ClientInput,
    PeerText,
    RetrievedText,
    ToolText,
    ModelDerived,
}

pub type Zaufanie = TrustLevel;

/// Data sink destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataSink {
    #[serde(alias = "argument_akcji")]
    ActionArgument,
    #[serde(alias = "instrukcja_kolejnego_zlecenia")]
    NextJobInstruction,
    SystemPrompt,
    Log,
}

pub type Sink = DataSink;

pub use DataSink::{
    ActionArgument as ArgumentAkcji, NextJobInstruction as InstrukcjaKolejnegoZlecenia,
};

/// Whether flow from source to sink is permitted without explicit declassification.
pub fn allowed_without_declassification(source: TrustLevel, sink: DataSink) -> bool {
    use DataSink::*;
    use TrustLevel::*;
    match sink {
        Log => true,
        ActionArgument | NextJobInstruction | SystemPrompt => matches!(source, Operator),
    }
}

pub use allowed_without_declassification as dozwolony_bez_deklasacji;

/// Whether flow requires operator declassification.
pub fn requires_declassification(source: TrustLevel, sink: DataSink) -> bool {
    !allowed_without_declassification(source, sink)
}

pub use requires_declassification as wymaga_deklasacji;

/// Evaluates if flow is permitted either by default or by explicit declassifications.
pub fn flow_ok(source: TrustLevel, sink: DataSink, declassifications: &[(TrustLevel, DataSink)]) -> bool {
    allowed_without_declassification(source, sink)
        || declassifications
            .iter()
            .any(|(z, s)| *z == source && *s == sink)
}

pub use flow_ok as przeplyw_ok;

#[cfg(test)]
mod tests {
    use super::*;
    use DataSink::*;
    use TrustLevel::*;

    #[test]
    fn untrusted_sources_denied_by_default() {
        assert!(!flow_ok(PeerText, NextJobInstruction, &[]));
        assert!(!flow_ok(RetrievedText, ActionArgument, &[]));
        assert!(!flow_ok(ToolText, ActionArgument, &[]));
        assert!(!flow_ok(ModelDerived, ActionArgument, &[]));
        assert!(!flow_ok(RetrievedText, SystemPrompt, &[]));
        assert!(!flow_ok(PeerText, SystemPrompt, &[]));
    }

    #[test]
    fn operator_writes_to_all_sinks_and_log_accepts_all() {
        for s in [ActionArgument, NextJobInstruction, SystemPrompt, Log] {
            assert!(flow_ok(Operator, s, &[]), "operator -> {s:?}");
        }
        for z in [Operator, ClientInput, PeerText, RetrievedText, ToolText, ModelDerived] {
            assert!(flow_ok(z, Log, &[]), "{z:?} -> Log");
        }
    }

    #[test]
    fn explicit_operator_declassification_opens_specific_flow() {
        let dekl = vec![(RetrievedText, ActionArgument)];
        assert!(flow_ok(RetrievedText, ActionArgument, &dekl));
        assert!(!flow_ok(RetrievedText, NextJobInstruction, &dekl));
        assert!(!flow_ok(ToolText, ActionArgument, &dekl));
    }
}

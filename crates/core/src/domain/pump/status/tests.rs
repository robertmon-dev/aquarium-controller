use crate::domain::pump::*;

#[test]
fn operation_states_follow_phases() {
    assert_eq!(
        OperationState::from(Phase::Draining),
        OperationState::Draining
    );
    assert_eq!(
        OperationState::from(Phase::Settling),
        OperationState::Settling
    );
    assert_eq!(
        OperationState::from(Phase::Refilling),
        OperationState::Refilling
    );
}

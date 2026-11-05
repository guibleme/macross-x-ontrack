//! Pure command transitions. External effects are never performed by this reducer.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum State {
    Recorded,
    Dispatched,
    Confirmed,
    Failed,
    Unknown,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Event {
    Dispatch,
    Confirm,
    LoseOutcome,
    ConfirmReadback,
    RejectNoEffect,
}

pub(crate) fn transition(state: State, event: Event) -> io::Result<State> {
    match (state, event) {
        (State::Recorded, Event::Dispatch) => Ok(State::Dispatched),
        (State::Dispatched, Event::Confirm) => Ok(State::Confirmed),
        (State::Dispatched, Event::RejectNoEffect) => Ok(State::Failed),
        (State::Recorded | State::Dispatched, Event::LoseOutcome) => Ok(State::Unknown),
        (State::Unknown, Event::ConfirmReadback) => Ok(State::Confirmed),
        _ => Err(io::Error::other("Invalid command transition")),
    }
}

pub(crate) fn advance(operation: &mut Value, event: Event) -> io::Result<()> {
    let state: State = serde_json::from_value(operation["state"].clone())?;
    operation["state"] = serde_json::to_value(transition(state, event)?)?;
    Ok(())
}

pub(crate) fn validate(journal: &Value) -> io::Result<()> {
    if journal["version"] != 1 {
        return Err(io::Error::other("Unsupported command journal version"));
    }
    let operations = journal["operations"]
        .as_object()
        .ok_or_else(|| io::Error::other("Invalid command journal"))?;
    for operation in operations.values() {
        serde_json::from_value::<State>(operation["state"].clone())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_state_event_pair_is_defined_or_rejected() {
        let states = [
            State::Recorded,
            State::Dispatched,
            State::Confirmed,
            State::Failed,
            State::Unknown,
        ];
        let events = [
            Event::Dispatch,
            Event::Confirm,
            Event::LoseOutcome,
            Event::ConfirmReadback,
            Event::RejectNoEffect,
        ];
        let mut permitted = 0;
        for state in states {
            for event in events {
                if let Ok(next) = transition(state, event) {
                    permitted += 1;
                    assert!(!matches!(state, State::Confirmed | State::Failed));
                    assert_ne!(next, State::Recorded);
                }
            }
        }
        assert_eq!(permitted, 6);
        assert_eq!(
            transition(State::Unknown, Event::ConfirmReadback).unwrap(),
            State::Confirmed
        );
        assert!(transition(State::Unknown, Event::Dispatch).is_err());
    }
    #[test]
    fn future_states_or_versions_fail_closed_without_rewriting_the_input() {
        let journal = serde_json::json!({"version":1,"operations":{"x":{"state":"future"}}});
        let before = journal.clone();
        assert!(validate(&journal).is_err());
        assert_eq!(journal, before);
        assert!(validate(&serde_json::json!({"version":2,"operations":{}})).is_err());
    }
}

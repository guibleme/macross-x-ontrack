import { check } from "./errors.mjs";
export const MACHINES = Object.freeze({
  planning: {
    states: ["draft", "approved", "withdrawn"],
    events: ["approve", "withdraw"],
    transitions: {
      draft: { approve: "approved", withdraw: "withdrawn" },
      approved: { withdraw: "withdrawn" },
      withdrawn: {},
    },
  },
  dependencies: {
    states: ["unknown", "pending", "satisfied", "invalid"],
    events: [
      "observe_pending",
      "observe_satisfied",
      "observe_invalid",
      "observe_unknown",
    ],
    transitions: Object.fromEntries(
      ["unknown", "pending", "satisfied", "invalid"].map((state) => [
        state,
        {
          observe_pending: "pending",
          observe_satisfied: "satisfied",
          observe_invalid: "invalid",
          observe_unknown: "unknown",
        },
      ]),
    ),
  },
});
// This reducer owns only Macross states. Execution and result acceptance belong to OnTrack.
export function transition(machine, state, event, facts = {}) {
  const definition = MACHINES[machine];
  check(definition, "unknown_machine");
  check(definition.states.includes(state), "unknown_state");
  check(definition.events.includes(event), "unknown_event");
  const next = definition.transitions[state]?.[event];
  check(next, "invalid_transition", 409);
  if (machine === "planning" && event === "approve") {
    check(facts.criteriaValid === true, "criteria_required", 409);
    check(facts.dependenciesValid === true, "invalid_dependencies", 409);
  }
  if (machine === "planning" && event === "withdraw")
    check(facts.dispatched !== true, "active_wave_cannot_withdraw", 409);
  return next;
}
export function dependencyState(dependencies) {
  return transition(
    "dependencies",
    "unknown",
    dependencies.some((d) => !d || d.planning === "withdrawn")
      ? "observe_invalid"
      : dependencies.every(
            (d) =>
              d.review === "accepted" &&
              d.reviewValidity === "current" &&
              d.integration === "integrated",
          )
        ? "observe_satisfied"
        : dependencies.some((d) => d.sourceStatus === "unavailable" && d.beadId)
          ? "observe_unknown"
          : "observe_pending",
  );
}

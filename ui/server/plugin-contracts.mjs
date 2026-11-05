import specification from "../../contracts/connectors-v1.json" with { type: "json" };
import { AppError, check, identifier } from "./errors.mjs";

export const CONNECTOR_PROTOCOL_VERSION = 1;
export const CONNECTOR_CONTRACT = specification;

export function validateDefinitions(definitions) {
  check(Array.isArray(definitions), "invalid_plugin_registry");
  const seen = new Set();
  for (const definition of definitions) {
    check(
      definition?.protocolVersion === CONNECTOR_PROTOCOL_VERSION,
      "incompatible_plugin_version",
      409,
    );
    identifier(definition.id);
    check(!seen.has(definition.id), "duplicate_plugin");
    seen.add(definition.id);
    check(
      Array.isArray(definition.contracts) && definition.contracts.length > 0,
      "plugin_contract_required",
    );
    check(
      new Set(definition.contracts).size === definition.contracts.length,
      "duplicate_plugin_contract",
    );
    for (const contract of definition.contracts)
      check(
        Object.hasOwn(specification.requiredOperations, contract),
        "unknown_plugin_contract",
      );
    check(Array.isArray(definition.operations), "plugin_operations_required");
    for (const contract of definition.contracts) {
      for (const operation of specification.requiredOperations[contract])
        check(
          definition.operations.includes(operation),
          "plugin_operation_missing",
        );
    }
    for (const operation of definition.operations) {
      check(
        Object.hasOwn(specification.operations, operation),
        "unknown_plugin_operation",
      );
      check(
        definition.contracts.includes(specification.operations[operation].kind),
        "wrong_plugin_contract",
      );
    }
    check(
      Array.isArray(definition.capabilities) &&
        Array.isArray(definition.fields),
      "invalid_plugin_descriptor",
    );
  }
  return definitions;
}

export function requireConnectorOperation(definition, operation) {
  check(
    definition.protocolVersion === CONNECTOR_PROTOCOL_VERSION,
    "incompatible_plugin_version",
    409,
  );
  if (!definition.operations.includes(operation))
    throw new AppError("capability_unavailable", 409);
  return specification.operations[operation];
}

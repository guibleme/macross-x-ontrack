export class AppError extends Error {
  constructor(code, status = 400) {
    super(code);
    this.code = code;
    this.status = status;
  }
}
export function check(ok, code = "invalid_input", status = 400) {
  if (!ok) throw new AppError(code, status);
}
export function text(value, max = 4000) {
  check(
    typeof value === "string" &&
      value.trim().length > 0 &&
      Buffer.byteLength(value) <= max &&
      !value.includes("\0"),
  );
  return value.trim();
}
export function identifier(value) {
  check(
    typeof value === "string" &&
      /^[a-zA-Z0-9][a-zA-Z0-9._:-]{0,127}$/.test(value),
    "invalid_id",
  );
  return value;
}
export function object(value, keys) {
  check(value && typeof value === "object" && !Array.isArray(value));
  check(
    Object.keys(value).every((key) => keys.includes(key)),
    "unknown_field",
  );
  return value;
}
export function errorCode(error) {
  return error instanceof AppError ? error.code : "operation_unavailable";
}

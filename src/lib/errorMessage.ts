/** Tauri rejects commands with plain { kind, message, detail } objects. */
export function errorMessage(error: unknown): string {
  if (error && typeof error === "object" && "message" in error && typeof error.message === "string") return error.message;
  if (typeof error === "string") return error;
  return "Something went wrong. Please try again.";
}

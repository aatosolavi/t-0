const host = location.hostname;
const local =
  host === "localhost" || host === "127.0.0.1" || host.endsWith(".localhost");

if (local) {
  await import("./agentation.bundle.js");
}

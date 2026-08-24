import { createElement } from "react";
import { createRoot } from "react-dom/client";
import { Agentation } from "agentation";

const host = location.hostname;
const local =
  host === "localhost" || host === "127.0.0.1" || host.endsWith(".localhost");

if (local && !document.getElementById("agentation-root")) {
  const root = document.createElement("div");
  root.id = "agentation-root";
  document.body.appendChild(root);
  createRoot(root).render(
    createElement(Agentation, { endpoint: "http://localhost:4747" }),
  );
}

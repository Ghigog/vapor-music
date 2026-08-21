import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { restoreAppearance } from "./lib/theme";
import "./app.css";

// Before the first paint, not in an effect: the theme is remembered locally
// precisely so it does not have to wait for React, let alone for the backend.
restoreAppearance();

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

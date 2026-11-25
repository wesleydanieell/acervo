import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./design-system.css";
import "./form-controls.css";
import "./appearance-surfaces.css";
import "./motion.css";
import "./appearance-transition.css";
import "./popup.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

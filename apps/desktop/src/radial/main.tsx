import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "../shared/base.css";
import "./radial.css";
import { RadialMenu } from "./RadialMenu";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <RadialMenu />
  </StrictMode>,
);

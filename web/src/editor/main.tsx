import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./editor.css";

const container = document.querySelector("#root");
if (container) createRoot(container).render(<App />);

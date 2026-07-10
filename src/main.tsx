import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { PRODUCT_NAME } from "./lib/product";
import "./styles/tokens.css";

document.title = PRODUCT_NAME;

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);

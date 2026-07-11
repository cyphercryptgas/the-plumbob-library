import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { PRODUCT_NAME } from "./lib/product";
import "@fontsource/playfair-display/600.css";
import "@fontsource/playfair-display/700.css";
import "./styles/tokens.css";

document.title = PRODUCT_NAME;

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);

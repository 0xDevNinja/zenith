import "./polyfills";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ThemeProvider } from "./lib/theme";
import { SolanaProviders } from "./providers/SolanaProviders";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ThemeProvider>
      <SolanaProviders>
        <App />
      </SolanaProviders>
    </ThemeProvider>
  </React.StrictMode>,
);

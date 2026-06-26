import "./polyfills";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { ThemeProvider } from "./lib/theme";
import { ToastProvider } from "./lib/toast";
import { SolanaProviders } from "./providers/SolanaProviders";
import { Toaster } from "./components/Toaster";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ThemeProvider>
      <ToastProvider>
        <SolanaProviders>
          <App />
          <Toaster />
        </SolanaProviders>
      </ToastProvider>
    </ThemeProvider>
  </React.StrictMode>,
);

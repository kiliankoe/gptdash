import React from "react";
import ReactDOM from "react-dom/client";
import { createBrowserRouter, RouterProvider } from "react-router-dom";
import App from "./App";
import Home from "./pages/Home";
import Host from "./pages/Host";
import Lobby from "./pages/Lobby";
import Play from "./pages/Play";

const router = createBrowserRouter([
  {
    path: "/",
    element: <App />,
    children: [
      { index: true, element: <Home /> },
      { path: "host", element: <Host /> },
      { path: "lobby/:code", element: <Lobby /> },
      { path: "host/:code", element: <Host /> },
      { path: "play/:code", element: <Play /> },
    ],
  },
]);

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <RouterProvider router={router} />
  </React.StrictMode>,
);

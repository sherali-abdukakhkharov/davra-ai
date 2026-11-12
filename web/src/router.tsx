import { createBrowserRouter, Outlet, ScrollRestoration } from "react-router";
import { PageLoading } from "@/components/site/Loading";
import Landing from "@/pages/Landing";

function Root() {
  return (
    <>
      <ScrollRestoration />
      <Outlet />
    </>
  );
}

// The landing page ships in the main bundle; everything behind Login is split out.
export const router = createBrowserRouter([
  {
    element: <Root />,
    HydrateFallback: () => <PageLoading />,
    children: [
      { index: true, element: <Landing /> },
      { path: "login", lazy: async () => ({ Component: (await import("@/pages/Login")).default }) },
      { path: "register", lazy: async () => ({ Component: (await import("@/pages/Register")).default }) },
      {
        path: "app",
        lazy: async () => {
          const m = await import("@/pages/AppHome");
          return { Component: m.default, loader: m.loader };
        },
      },
      { path: "*", lazy: async () => ({ Component: (await import("@/pages/NotFound")).default }) },
    ],
  },
]);

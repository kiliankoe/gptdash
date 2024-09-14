"use client";

import { QueryClient, QueryClientProvider } from "react-query";
import { GameProvider } from "./components/GameProvider";

export const queryClient = new QueryClient();

export default function ClientLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <QueryClientProvider client={queryClient}>
      <GameProvider>{children}</GameProvider>
    </QueryClientProvider>
  );
}

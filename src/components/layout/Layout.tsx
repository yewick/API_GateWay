import type { ReactNode } from "react";
import { Sidebar } from "./Sidebar";
import { GatewayStatusBanner } from "./GatewayStatusBanner";

export const Layout = ({ children }: { children: ReactNode }) => {
  return (
    <div className="flex h-screen overflow-hidden">
      <Sidebar />
      <main className="flex-1 overflow-y-auto">
        <div className="max-w-[1400px] mx-auto p-6 lg:p-8">
          <GatewayStatusBanner />
          {children}
        </div>
      </main>
    </div>
  );
};

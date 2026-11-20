import { useState } from "react";
import { PageHeader } from "../components/ui/PageHeader";
import { Tabs } from "../components/ui/Tabs";
import { ClaudeSetupSection } from "../components/client-access/ClaudeSetupSection";
import { CodexSetupSection } from "../components/client-access/CodexSetupSection";
import { KeepAliveNotice } from "../components/client-access/KeepAliveNotice";
import { useClientSetupEvents } from "../hooks/useClientSetup";
import type { ClientKind } from "../types";

export const ClientAccessPage = () => {
  const [client, setClient] = useState<ClientKind>("codex");
  useClientSetupEvents();

  return (
    <div>
      <PageHeader
        title="客户端接入"
        description="选择或新建一个 YeAPI 密钥，一键写入本机客户端配置，无需登录上游账号"
      />

      <KeepAliveNotice />

      <Tabs
        tabs={[
          { key: "codex", label: "Codex" },
          { key: "claude", label: "Claude Code" },
        ]}
        activeKey={client}
        onChange={(key) => setClient(key as ClientKind)}
        className="mb-5"
      />

      {client === "codex" ? <CodexSetupSection /> : <ClaudeSetupSection />}
    </div>
  );
};

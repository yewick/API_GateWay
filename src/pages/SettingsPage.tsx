import { useState } from "react";
import { Server, SlidersHorizontal, Palette, RotateCcw, Save } from "lucide-react";
import { PageHeader } from "../components/ui/PageHeader";
import { Tabs, type TabItem } from "../components/ui/Tabs";
import { Button } from "../components/ui/Button";
import { ServiceConfigTab } from "../components/settings/ServiceConfigTab";
import { GeneralTab } from "../components/settings/GeneralTab";
import { UISettingsTab } from "../components/settings/UISettingsTab";
import { RetryStrategyTab } from "../components/settings/RetryStrategyTab";
import { useSettingsStore } from "../stores/settingsStore";
import { toast } from "../lib/toast";

const tabs: TabItem[] = [
  { key: "service", label: "服务配置", icon: <Server size={15} /> },
  { key: "general", label: "通用设置", icon: <SlidersHorizontal size={15} /> },
  { key: "ui", label: "界面设置", icon: <Palette size={15} /> },
  { key: "retry", label: "重试策略", icon: <RotateCcw size={15} /> },
];

export const SettingsPage = () => {
  const [activeTab, setActiveTab] = useState("service");
  const saveSettings = useSettingsStore((s) => s.saveSettings);
  const [saving, setSaving] = useState(false);

  const handleSave = async () => {
    setSaving(true);
    try {
      await saveSettings();
      toast.success("保存成功", "设置已保存");
    } catch (err) {
      toast.error("保存失败", (err as Error)?.message);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div>
      <PageHeader
        title="设置"
        description="配置网关服务与应用行为"
        actions={
          <Button onClick={handleSave} loading={saving}>
            <Save size={16} />
            保存设置
          </Button>
        }
      />

      <div className="bg-bg-secondary border border-border-primary rounded-xl overflow-hidden">
        <div className="px-4 pt-3">
          <Tabs tabs={tabs} activeKey={activeTab} onChange={setActiveTab} />
        </div>
        <div className="p-6">
          {activeTab === "service" && <ServiceConfigTab />}
          {activeTab === "general" && <GeneralTab />}
          {activeTab === "ui" && <UISettingsTab />}
          {activeTab === "retry" && <RetryStrategyTab />}
        </div>
      </div>
    </div>
  );
};

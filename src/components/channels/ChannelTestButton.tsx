import { PlugZap } from "lucide-react";
import { useTestChannel } from "../../hooks/useChannels";
import { Button } from "../ui/Button";
import { toast } from "../../lib/toast";

interface ChannelTestButtonProps {
  channelId: string;
  channelName: string;
  size?: "sm" | "md";
}

export function ChannelTestButton({
  channelId,
  channelName,
  size = "sm",
}: ChannelTestButtonProps) {
  const mutation = useTestChannel();

  const handleTest = async () => {
    try {
      const result = await mutation.mutateAsync(channelId);
      if (result.success) {
        toast.success(
          "测试成功",
          `渠道「${channelName}」连通正常，延迟 ${result.latency_ms}ms`,
        );
      } else {
        toast.error(
          "测试失败",
          result.error_message ?? `渠道「${channelName}」连接异常`,
        );
      }
    } catch (err) {
      toast.error("测试失败", (err as Error)?.message ?? "未知错误");
    }
  };

  return (
    <Button
      variant="secondary"
      size={size}
      onClick={handleTest}
      loading={mutation.isPending}
      title="测试渠道连通性"
    >
      <PlugZap size={14} />
      测试
    </Button>
  );
}

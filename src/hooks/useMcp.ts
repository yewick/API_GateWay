import { useQuery } from "@tanstack/react-query";
import { mcpApi } from "../lib/api";

export const mcpKeys = {
  statuses: ["mcp", "statuses"] as const,
};

export const useServiceStatuses = () =>
  useQuery({
    queryKey: mcpKeys.statuses,
    queryFn: () => mcpApi.getStatuses(),
  });

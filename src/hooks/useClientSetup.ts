import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { clientSetupApi } from "../lib/api";
import type { ApplyClientInput, ClientKind } from "../types";
import { apiKeyKeys } from "./useApiKeys";
import { dashboardKeys } from "./useDashboard";

export const clientSetupKeys = {
  all: ["client-setup"] as const,
  status: (client: ClientKind) => ["client-setup", "status", client] as const,
  models: ["client-setup", "models"] as const,
  preview: (model: string) => ["client-setup", "preview", model] as const,
};

export const useClientSetupStatus = (client: ClientKind) =>
  useQuery({
    queryKey: clientSetupKeys.status(client),
    queryFn: () => clientSetupApi.status(client),
  });

export const useClientSetupModels = () =>
  useQuery({
    queryKey: clientSetupKeys.models,
    queryFn: () => clientSetupApi.listModels(),
    staleTime: 60_000,
  });

export const useClientSetupPreview = (input: ApplyClientInput | null) =>
  useQuery({
    queryKey: clientSetupKeys.preview(input?.model ?? ""),
    queryFn: () => clientSetupApi.preview(input as ApplyClientInput),
    enabled: !!input && input.model.trim().length > 0,
  });

export const useApplyClientSetup = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: ApplyClientInput) => clientSetupApi.apply(input),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: clientSetupKeys.status("codex") });
      qc.invalidateQueries({ queryKey: apiKeyKeys.all });
      qc.invalidateQueries({ queryKey: dashboardKeys.stats });
    },
  });
};

export const useDisconnectClientSetup = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (client: ClientKind) => clientSetupApi.disconnect(client),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: clientSetupKeys.status("codex") });
      qc.invalidateQueries({ queryKey: apiKeyKeys.all });
      qc.invalidateQueries({ queryKey: dashboardKeys.stats });
    },
  });
};

export const useTestClientSetup = () =>
  useMutation({
    mutationFn: (client: ClientKind) => clientSetupApi.test(client),
  });

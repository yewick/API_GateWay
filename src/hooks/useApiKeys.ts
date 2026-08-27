import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { apiKeyApi } from "../lib/api";
import type { CreateApiKeyInput } from "../types";
import { dashboardKeys } from "./useDashboard";

export const apiKeyKeys = {
  all: ["api-keys"] as const,
};

export const useApiKeys = () =>
  useQuery({
    queryKey: apiKeyKeys.all,
    queryFn: () => apiKeyApi.getAll(),
  });

export const useCreateApiKey = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: CreateApiKeyInput) => apiKeyApi.create(input),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: apiKeyKeys.all });
      qc.invalidateQueries({ queryKey: dashboardKeys.stats });
    },
  });
};

export const useUpdateApiKey = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, status }: { id: string; status: number }) =>
      apiKeyApi.update(id, status),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: apiKeyKeys.all });
      qc.invalidateQueries({ queryKey: dashboardKeys.stats });
    },
  });
};

export const useDeleteApiKey = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => apiKeyApi.delete(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: apiKeyKeys.all });
      qc.invalidateQueries({ queryKey: dashboardKeys.stats });
    },
  });
};

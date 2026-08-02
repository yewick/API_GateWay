import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { channelApi } from "../lib/api";
import type { CreateChannelInput, TestChannelResult, UpdateChannelInput } from "../types";

export const channelKeys = {
  all: ["channels"] as const,
  detail: (id: string) => ["channels", id] as const,
};

// ===== 查询 =====

export const useChannels = () =>
  useQuery({
    queryKey: channelKeys.all,
    queryFn: () => channelApi.getAll(),
  });

export const useChannel = (id: string | null) =>
  useQuery({
    queryKey: channelKeys.detail(id ?? ""),
    queryFn: () => channelApi.get(id ?? ""),
    enabled: !!id,
  });

// ===== 变更 =====

export const useCreateChannel = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: CreateChannelInput) => channelApi.create(input),
    onSuccess: () => qc.invalidateQueries({ queryKey: channelKeys.all }),
  });
};

export const useUpdateChannel = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: UpdateChannelInput) => channelApi.update(input),
    onSuccess: (_, input) => {
      qc.invalidateQueries({ queryKey: channelKeys.all });
      qc.invalidateQueries({ queryKey: channelKeys.detail(input.id) });
    },
  });
};

export const useDeleteChannel = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => channelApi.delete(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: channelKeys.all }),
  });
};

export const useToggleChannel = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, status }: { id: string; status: number }) =>
      channelApi.toggle(id, status),
    onSuccess: () => qc.invalidateQueries({ queryKey: channelKeys.all }),
  });
};

export const useTestChannel = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string): Promise<TestChannelResult> => channelApi.test(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: channelKeys.all }),
  });
};

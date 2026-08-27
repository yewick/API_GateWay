import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { knowledgeApi } from "../lib/api";
import type {
  ConversationMessage,
  CreateKbInput,
  ImportSourceInput,
  UpdateKbInput,
} from "../types";

export const knowledgeKeys = {
  all: ["knowledge"] as const,
  list: () => ["knowledge", "list"] as const,
  detail: (id: string) => ["knowledge", "detail", id] as const,
  documents: (kbId: string) => ["knowledge", "documents", kbId] as const,
  documentContent: (kbId: string, docId: string) =>
    ["knowledge", "documents", kbId, docId, "content"] as const,
  documentChunks: (kbId: string, docId: string) =>
    ["knowledge", "documents", kbId, docId, "chunks"] as const,
  conversations: (kbId: string) => ["knowledge", "conversations", kbId] as const,
  sources: (kbId: string) => ["knowledge", "sources", kbId] as const,
  index: (kbId: string) => ["knowledge", "index", kbId] as const,
  stats: (kbId: string) => ["knowledge", "stats", kbId] as const,
};

// ===== 查询 =====

export const useKnowledgeBases = () =>
  useQuery({
    queryKey: knowledgeKeys.list(),
    queryFn: () => knowledgeApi.list(),
  });

export const useKnowledgeBase = (id: string | null) =>
  useQuery({
    queryKey: knowledgeKeys.detail(id ?? ""),
    queryFn: () => knowledgeApi.get(id ?? ""),
    enabled: !!id,
  });

export const useKbDocuments = (kbId: string | null) =>
  useQuery({
    queryKey: knowledgeKeys.documents(kbId ?? ""),
    queryFn: () => knowledgeApi.listDocuments(kbId ?? ""),
    enabled: !!kbId,
  });

export const useKbDocumentContent = (kbId: string | null, docId: string | null) =>
  useQuery({
    queryKey: knowledgeKeys.documentContent(kbId ?? "", docId ?? ""),
    queryFn: () => knowledgeApi.getDocumentContent(kbId ?? "", docId ?? ""),
    enabled: !!kbId && !!docId,
  });

export const useKbDocumentChunks = (kbId: string | null, docId: string | null) =>
  useQuery({
    queryKey: knowledgeKeys.documentChunks(kbId ?? "", docId ?? ""),
    queryFn: () => knowledgeApi.listDocumentChunks(kbId ?? "", docId ?? ""),
    enabled: !!kbId && !!docId,
  });

export const useKbConversations = (kbId: string | null) =>
  useQuery({
    queryKey: knowledgeKeys.conversations(kbId ?? ""),
    queryFn: () => knowledgeApi.getConversations(kbId ?? ""),
    enabled: !!kbId,
  });

export const useKbSources = (kbId: string | null) =>
  useQuery({
    queryKey: knowledgeKeys.sources(kbId ?? ""),
    queryFn: () => knowledgeApi.listSources(kbId ?? ""),
    enabled: !!kbId,
  });

export const useKbIndex = (kbId: string | null) =>
  useQuery({
    queryKey: knowledgeKeys.index(kbId ?? ""),
    queryFn: () => knowledgeApi.getIndex(kbId ?? ""),
    enabled: !!kbId,
  });

export const useKbStats = (kbId: string | null) =>
  useQuery({
    queryKey: knowledgeKeys.stats(kbId ?? ""),
    queryFn: () => knowledgeApi.getStats(kbId ?? ""),
    enabled: !!kbId,
  });

// ===== 变更 =====

const invalidateKb = (qc: ReturnType<typeof useQueryClient>, kbId?: string) => {
  qc.invalidateQueries({ queryKey: knowledgeKeys.list() });
  if (kbId) {
    qc.invalidateQueries({ queryKey: knowledgeKeys.documents(kbId) });
    qc.invalidateQueries({ queryKey: knowledgeKeys.stats(kbId) });
    qc.invalidateQueries({ queryKey: knowledgeKeys.detail(kbId) });
  }
};

export const useCreateKnowledgeBase = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (input: CreateKbInput) => knowledgeApi.create(input),
    onSuccess: () => qc.invalidateQueries({ queryKey: knowledgeKeys.list() }),
  });
};

export const useUpdateKnowledgeBase = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, input }: { id: string; input: UpdateKbInput }) =>
      knowledgeApi.update(id, input),
    onSuccess: (_, { id }) => {
      qc.invalidateQueries({ queryKey: knowledgeKeys.list() });
      qc.invalidateQueries({ queryKey: knowledgeKeys.detail(id) });
    },
  });
};

export const useDeleteKnowledgeBase = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => knowledgeApi.remove(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: knowledgeKeys.list() }),
  });
};

export const useUploadDocument = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ kbId, path }: { kbId: string; path: string }) =>
      knowledgeApi.uploadDocument(kbId, path),
    onSuccess: (_, { kbId }) => invalidateKb(qc, kbId),
  });
};

export const useIngestDocument = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ kbId, docId }: { kbId: string; docId: string }) =>
      knowledgeApi.ingestDocument(kbId, docId),
    onSuccess: (_, { kbId }) => invalidateKb(qc, kbId),
  });
};

export const useDeleteDocument = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ kbId, docId }: { kbId: string; docId: string }) =>
      knowledgeApi.deleteDocument(kbId, docId),
    onSuccess: (_, { kbId }) => invalidateKb(qc, kbId),
  });
};

export const useAskKnowledgeBase = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      kbId,
      question,
      options,
    }: {
      kbId: string;
      question: string;
      options?: { model?: string; topK?: number; history?: ConversationMessage[]; apiKeyId?: string };
    }) => knowledgeApi.ask(kbId, question, options),
    onSuccess: (_, { kbId }) =>
      qc.invalidateQueries({ queryKey: knowledgeKeys.conversations(kbId) }),
  });
};

export const useClearConversations = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (kbId: string) => knowledgeApi.clearConversations(kbId),
    onSuccess: (_, kbId) =>
      qc.invalidateQueries({ queryKey: knowledgeKeys.conversations(kbId) }),
  });
};

export const useSearchKb = () =>
  useMutation({
    mutationFn: ({
      kbId,
      query,
      topK,
      symbolKind,
    }: {
      kbId: string;
      query: string;
      topK?: number;
      symbolKind?: string;
    }) => knowledgeApi.search(kbId, query, topK, symbolKind),
  });

export const useBuildIndex = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (kbId: string) => knowledgeApi.buildIndex(kbId),
    onSuccess: (_, kbId) =>
      qc.invalidateQueries({ queryKey: knowledgeKeys.index(kbId) }),
  });
};

export const useImportSource = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ kbId, input }: { kbId: string; input: ImportSourceInput }) =>
      knowledgeApi.importSource(kbId, input),
    onSuccess: (_, { kbId }) => {
      qc.invalidateQueries({ queryKey: knowledgeKeys.sources(kbId) });
      invalidateKb(qc, kbId);
    },
  });
};

export const useDeleteSource = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ kbId, sourceId }: { kbId: string; sourceId: string }) =>
      knowledgeApi.deleteSource(kbId, sourceId),
    onSuccess: (_, { kbId }) =>
      qc.invalidateQueries({ queryKey: knowledgeKeys.sources(kbId) }),
  });
};

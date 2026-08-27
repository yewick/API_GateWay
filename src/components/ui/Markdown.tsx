import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";
import type { Components } from "react-markdown";

/** Markdown 渲染样式（无 typography 插件，手工指定各元素样式） */
const mdComponents: Components = {
  h1: ({ children }) => (
    <h1 className="text-lg font-semibold text-text-primary mt-5 mb-2 first:mt-0">{children}</h1>
  ),
  h2: ({ children }) => (
    <h2 className="text-base font-semibold text-text-primary mt-5 mb-2 first:mt-0">{children}</h2>
  ),
  h3: ({ children }) => (
    <h3 className="text-sm font-semibold text-text-primary mt-4 mb-2 first:mt-0">{children}</h3>
  ),
  h4: ({ children }) => (
    <h4 className="text-sm font-medium text-text-primary mt-3 mb-1.5 first:mt-0">{children}</h4>
  ),
  p: ({ children }) => <p className="text-sm text-text-secondary leading-relaxed mb-3 last:mb-0">{children}</p>,
  ul: ({ children }) => <ul className="list-disc pl-5 mb-3 space-y-1">{children}</ul>,
  ol: ({ children }) => <ol className="list-decimal pl-5 mb-3 space-y-1">{children}</ol>,
  li: ({ children }) => <li className="text-sm text-text-secondary leading-relaxed">{children}</li>,
  a: ({ children, href }) => (
    <a href={href} target="_blank" rel="noreferrer" className="text-accent underline underline-offset-2">
      {children}
    </a>
  ),
  blockquote: ({ children }) => (
    <blockquote className="border-l-2 border-border-primary pl-3 my-3 text-text-muted">{children}</blockquote>
  ),
  table: ({ children }) => (
    <div className="overflow-x-auto mb-3">
      <table className="w-full text-xs border-collapse">{children}</table>
    </div>
  ),
  th: ({ children }) => (
    <th className="border border-border-primary bg-bg-tertiary px-2 py-1.5 text-left font-medium text-text-primary">
      {children}
    </th>
  ),
  td: ({ children }) => (
    <td className="border border-border-primary px-2 py-1.5 text-text-secondary">{children}</td>
  ),
  hr: () => <hr className="border-border-primary my-4" />,
  strong: ({ children }) => <strong className="font-semibold text-text-primary">{children}</strong>,
  code: ({ className, children, ...props }) => {
    const match = /language-(\w+)/.exec(className || "");
    const codeText = String(children).replace(/\n$/, "");
    if (match) {
      return (
        <SyntaxHighlighter style={oneDark} language={match[1]} PreTag="div" customStyle={{ borderRadius: 8, fontSize: 12, margin: "0 0 12px" }}>
          {codeText}
        </SyntaxHighlighter>
      );
    }
    return (
      <code className="px-1.5 py-0.5 rounded bg-bg-tertiary text-[13px] text-accent" {...props}>
        {children}
      </code>
    );
  },
};

/** 通用 Markdown 渲染组件，用于将问答回答 / 文档内容渲染为富文本。 */
export function MarkdownContent({ children }: { children: string }) {
  return (
    <ReactMarkdown remarkPlugins={[remarkGfm]} components={mdComponents}>
      {children}
    </ReactMarkdown>
  );
}

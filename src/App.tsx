import { lazy, Suspense } from "react";
import { Navigate, Route, Routes } from "react-router-dom";
import { StoreProvider } from "@/store";
import { Layout } from "@/components/Layout";
import { Skeleton } from "@/components/ui";
import { Home } from "@/pages/Home";
import { ProjectPrompts } from "@/pages/ProjectPrompts";
import { Export } from "@/pages/Export";
import { SearchResults } from "@/pages/SearchResults";
import { ExternalSessions } from "@/pages/ExternalSessions";

// 对话详情带着 Markdown 渲染与语法高亮，单独拆包按需加载
const ConversationDetail = lazy(() =>
  import("@/pages/ConversationDetail").then((module) => ({
    default: module.ConversationDetail,
  }))
);

// 外部会话详情复用同一套对话查看器，同样按需加载
const ExternalConversation = lazy(() =>
  import("@/pages/ExternalConversation").then((module) => ({
    default: module.ExternalConversation,
  }))
);

function ConversationFallback() {
  return (
    <div className="mx-auto max-w-6xl space-y-3 px-4 py-6 sm:px-6">
      {Array.from({ length: 4 }).map((_, index) => (
        <Skeleton key={index} className="h-28 w-full" />
      ))}
    </div>
  );
}

function ConversationRoute() {
  return (
    <Suspense fallback={<ConversationFallback />}>
      <ConversationDetail />
    </Suspense>
  );
}

function ExternalConversationRoute() {
  return (
    <Suspense fallback={<ConversationFallback />}>
      <ExternalConversation />
    </Suspense>
  );
}

export default function App() {
  return (
    <StoreProvider>
      <Routes>
        <Route element={<Layout />}>
          <Route index element={<Home />} />
          <Route path="export" element={<Export />} />
          <Route path="search" element={<SearchResults />} />
          <Route path="external" element={<ExternalSessions />} />
          <Route path="external/:id" element={<ExternalConversationRoute />} />
          <Route path="project/:encoded" element={<ProjectPrompts />} />
          <Route
            path="conversation/:agent/:sessionId"
            element={<ConversationRoute />}
          />
          <Route path="conversation/:sessionId" element={<ConversationRoute />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Route>
      </Routes>
    </StoreProvider>
  );
}

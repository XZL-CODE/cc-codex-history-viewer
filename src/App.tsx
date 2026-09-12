import { Navigate, Route, Routes } from "react-router-dom";
import { StoreProvider } from "@/store";
import { Layout } from "@/components/Layout";
import { Home } from "@/pages/Home";
import { ProjectPrompts } from "@/pages/ProjectPrompts";
import { ConversationDetail } from "@/pages/ConversationDetail";
import { Export } from "@/pages/Export";
import { SearchResults } from "@/pages/SearchResults";

export default function App() {
  return (
    <StoreProvider>
      <Routes>
        <Route element={<Layout />}>
          <Route index element={<Home />} />
          <Route path="export" element={<Export />} />
          <Route path="search" element={<SearchResults />} />
          <Route path="project/:encoded" element={<ProjectPrompts />} />
          <Route
            path="conversation/:agent/:sessionId"
            element={<ConversationDetail />}
          />
          <Route path="conversation/:sessionId" element={<ConversationDetail />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Route>
      </Routes>
    </StoreProvider>
  );
}

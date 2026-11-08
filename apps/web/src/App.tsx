import { BrowserRouter, Route, Routes } from "react-router-dom";
import { Layout } from "@/components/templates/Layout/Layout";
import { Dashboard } from "@/pages/Dashboard";
import { History } from "@/pages/History";
import { Lighting } from "@/pages/Lighting";
import { Settings } from "@/pages/Settings";
import { ErrorPage } from "@/pages/Error/Error";
import { AppProviders } from "@/providers/AppProviders";

export function App() {
  return (
    <AppProviders>
      <BrowserRouter>
        <Layout>
          <Routes>
            <Route path="/" element={<Dashboard />} />
            <Route path="/lighting" element={<Lighting />} />
            <Route path="/history" element={<History />} />
            <Route path="/settings" element={<Settings />} />
            <Route path="*" element={<ErrorPage code="not-found" />} />
          </Routes>
        </Layout>
      </BrowserRouter>
    </AppProviders>
  );
}

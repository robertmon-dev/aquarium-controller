import { BrowserRouter, Route, Routes } from "react-router-dom";
import { Layout } from "@/components/templates/Layout/Layout";
import { Dashboard } from "@/pages/Dashboard";
import { Lighting } from "@/pages/Lighting";
import { Settings } from "@/pages/Settings";
import { AppProviders } from "@/providers/AppProviders";

export function App() {
  return (
    <AppProviders>
      <BrowserRouter>
        <Layout>
          <Routes>
            <Route path="/" element={<Dashboard />} />
            <Route path="/lighting" element={<Lighting />} />
            <Route path="/history" element={<p>history — next plan</p>} />
            <Route path="/settings" element={<Settings />} />
          </Routes>
        </Layout>
      </BrowserRouter>
    </AppProviders>
  );
}

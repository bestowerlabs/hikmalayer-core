import { useState, useCallback } from "react";
import { BrowserRouter, Routes, Route } from "react-router-dom";
import { WalletProvider } from "./hooks/useWallet";
import { SignerProvider } from "./hooks/useSigner";
import { ExtensionProvider } from "./hooks/useExtension";
import { useWallet } from "./hooks/useWallet";

import Layout from "./components/Layout";
import DashboardPage from "./pages/DashboardPage";
import WalletPage from "./pages/WalletPage";
import ExplorerPage from "./pages/ExplorerPage";
import DexPage from "./pages/DexPage";
import CertificatesPage from "./pages/CertificatesPage";
import CredentialsPage from "./pages/CredentialsPage";
import StakingPage from "./pages/StakingPage";
import VestingPage from "./pages/VestingPage";
import GovernancePage from "./pages/GovernancePage";
import MiningPage from "./pages/MiningPage";

/// Shared refresh state so any page's action (a transfer, a stake deposit,
/// a swap) can trigger every other page's data to reload next time it's
/// visited, the same cross-page freshness the single-page layout used to
/// give for free by re-rendering everything at once.
const AppRoutes = () => {
  const { connectWallet } = useWallet();
  const [refreshTrigger, setRefreshTrigger] = useState(0);

  const handleUpdate = useCallback(() => {
    setRefreshTrigger((prev) => prev + 1);
  }, []);

  return (
    // One wallet identity for every route: the extension (preferred) and
    // the in-page wallet are shared context, so every page signs as the
    // same account rather than each keeping its own idea of who is
    // connected. This wraps the router, not a single page, so identity
    // persists across navigation.
    <ExtensionProvider>
      <SignerProvider onUnlock={connectWallet}>
        <Routes>
          <Route element={<Layout />}>
            <Route index element={<DashboardPage />} />
            <Route
              path="/wallet"
              element={<WalletPage refreshTrigger={refreshTrigger} onUpdate={handleUpdate} />}
            />
            <Route
              path="/explorer"
              element={<ExplorerPage refreshTrigger={refreshTrigger} onUpdate={handleUpdate} />}
            />
            <Route
              path="/dex"
              element={<DexPage refreshTrigger={refreshTrigger} onUpdate={handleUpdate} />}
            />
            <Route
              path="/staking"
              element={<StakingPage refreshTrigger={refreshTrigger} onUpdate={handleUpdate} />}
            />
            <Route
              path="/certificates"
              element={<CertificatesPage refreshTrigger={refreshTrigger} onUpdate={handleUpdate} />}
            />
            <Route path="/credentials" element={<CredentialsPage />} />
            <Route path="/vesting" element={<VestingPage />} />
            <Route path="/governance" element={<GovernancePage />} />
            <Route
              path="/mining"
              element={<MiningPage refreshTrigger={refreshTrigger} onUpdate={handleUpdate} />}
            />
          </Route>
        </Routes>
      </SignerProvider>
    </ExtensionProvider>
  );
};

const App = () => {
  return (
    <BrowserRouter>
      <WalletProvider>
        <AppRoutes />
      </WalletProvider>
    </BrowserRouter>
  );
};

export default App;

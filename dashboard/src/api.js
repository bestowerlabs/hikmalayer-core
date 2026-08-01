// src/api.js - Complete Hikmalayer API Integration
import axios from "axios";

// Which node this dashboard talks to.
//
// Resolution order, first hit wins:
//   1. `window.__HIKMALAYER_NODE__`  — set by a <script> the operator serves,
//      so one built bundle can be pointed at different nodes without a rebuild.
//   2. `VITE_API_BASE`              — baked in at build time.
//   3. the local devnet default.
//
// Whichever origin you choose must also appear in the node's
// CORS_ALLOWED_ORIGINS and in the dashboard's CSP `connect-src`.
const resolveApiBase = () => {
  const runtime =
    typeof window !== "undefined" ? window.__HIKMALAYER_NODE__ : undefined;
  const configured = runtime || import.meta.env?.VITE_API_BASE;
  return String(configured || "http://127.0.0.1:3000").replace(/\/+$/, "");
};

const API_BASE = resolveApiBase();

// Create axios instance with default config
const api = axios.create({
  baseURL: API_BASE,
  headers: {
    "Content-Type": "application/json",
  },
  timeout: 10000, // 10 second timeout
});

// Add response interceptor for error handling
api.interceptors.response.use(
  (response) => response,
  (error) => {
    console.error("API Error:", error);
    throw error;
  }
);

// Create a simple fetch wrapper for components that don't use axios
export const apiFetch = async (endpoint, options = {}) => {
  const url = `${API_BASE}${endpoint}`;

  try {
    const response = await fetch(url, {
      headers: {
        "Content-Type": "application/json",
        ...options.headers,
      },
      ...options,
    });

    if (!response.ok) {
      throw new Error(`HTTP error! status: ${response.status}`);
    }

    return response;
  } catch (error) {
    console.error(`API Error for ${endpoint}:`, error);
    throw error;
  }
};

// ===== AUTHENTICATION =====

export const generateNonce = async (address) => {
  return api.post("/auth/nonce", { address });
};

export const verifySignature = async (data) => {
  return api.post("/auth/verify", data);
};

export const logout = async () => {
  return api.delete("/auth/logout");
};

// ===== CERTIFICATE MANAGEMENT =====

export const issueCertificate = async (data) => {
  return api.post("/certificates/issue", data);
};

export const verifyCertificate = async (data) => {
  return api.post("/certificates/verify", data);
};

// ===== TOKEN MANAGEMENT =====

export const transferTokens = async (data) => {
  return api.post("/tokens/transfer", data);
};

export const getTokenBalance = async (account) => {
  return api.get(`/tokens/balance/${account}`);
};

// ===== NATIVE TOKEN STANDARD (HTS) =====

export const listAssets = async () => {
  return api.get("/assets");
};

export const getAsset = async (tokenId) => {
  return api.get(`/assets/${encodeURIComponent(tokenId)}`);
};

export const getAssetBalance = async (tokenId, account) => {
  return api.get(
    `/assets/${encodeURIComponent(tokenId)}/balance/${encodeURIComponent(account)}`
  );
};

export const createAsset = async (data) => {
  return api.post("/assets/create", data);
};

export const transferAsset = async (data) => {
  return api.post("/assets/transfer", data);
};

export const burnAsset = async (data) => {
  return api.post("/assets/burn", data);
};

// ===== NATIVE AMM DEX =====

export const listPools = async () => {
  return api.get("/dex/pools");
};

export const getPool = async (tokenId) => {
  return api.get(`/dex/pool/${encodeURIComponent(tokenId)}`);
};

export const getLpPosition = async (tokenId, account) => {
  return api.get(
    `/dex/position/${encodeURIComponent(tokenId)}/${encodeURIComponent(account)}`
  );
};

/// Read-only quote: the exact output the on-chain math would produce.
export const getSwapQuote = async (tokenId, direction, amountIn) => {
  return api.get(
    `/dex/quote/${encodeURIComponent(tokenId)}/${direction}/${amountIn}`
  );
};

export const submitSwap = async (data) => {
  return api.post("/dex/swap", data);
};

export const addLiquidity = async (data) => {
  return api.post("/dex/add", data);
};

export const removeLiquidity = async (data) => {
  return api.post("/dex/remove", data);
};

/// The network this node is on. Every signature must be scoped to it.
export const getChainId = async () => {
  const res = await api.get("/blockchain/state");
  return res.data?.chain_id ?? null;
};

export const getAccountNonce = async (account) => {
  return api.get(`/tokens/nonce/${encodeURIComponent(account)}`);
};

// ===== BLOCKCHAIN OPERATIONS =====

export const getBlocks = async () => {
  return api.get("/blocks");
};

export const getBlockByIndex = async (index) => {
  return api.get(`/blocks/${index}`);
};

export const getBlockchainStats = async () => {
  return api.get("/blockchain/stats");
};

// ===== EXPLORER (MODERN) =====

export const getExplorerOverview = async () => {
  return api.get("/explorer/overview");
};

export const getExplorerBlocks = async ({ offset = 0, limit = 20 } = {}) => {
  return api.get("/explorer/blocks", { params: { offset, limit } });
};

export const getExplorerBlockByIndex = async (index) => {
  return api.get(`/explorer/blocks/index/${index}`);
};

export const getExplorerBlockByHash = async (hash) => {
  return api.get(`/explorer/blocks/hash/${encodeURIComponent(hash)}`);
};

export const searchExplorer = async (query) => {
  return api.get(`/explorer/search/${encodeURIComponent(query)}`);
};

export const getPendingTransactionsStructured = async () => {
  return api.get("/explorer/transactions/pending");
};

// ===== MINING OPERATIONS =====

export const mineBlock = async () => {
  return api.post("/mine");
};

export const getMiningDifficulty = async () => {
  return api.get("/mining/difficulty");
};

export const setMiningDifficulty = async (data) => {
  return api.post("/mining/difficulty", data);
};

// ===== VALIDATION & SECURITY =====

export const validateBlockchain = async () => {
  return api.get("/blockchain/validate");
};

export const validateBlock = async (index) => {
  return api.get(`/blocks/${index}/validate`);
};

export const validateChain = async () => {
  return api.get("/validate");
};

// ===== TRANSACTION MANAGEMENT =====

export const getPendingTransactions = async () => {
  return api.get("/transactions/pending");
};

// ===== HELPER FUNCTIONS =====

// Complete workflow helper
export const completeWorkflow = async () => {
  try {
    // 1. Check blockchain status
    const stats = await getBlockchainStats();
    console.log("Blockchain Stats:", stats.data);

    // 2. Issue certificate
    const cert = await issueCertificate({
      id: "AUTO_CERT_" + Date.now(),
      issued_to: "test@example.com",
      description: "Automated Test Certificate",
    });
    console.log("Certificate issued:", cert.data);

    // 3. Transfer tokens
    const transfer = await transferTokens({
      from: "admin",
      to: "test@example.com",
      amount: 50,
    });
    console.log("Tokens transferred:", transfer.data);

    // 4. Mine block
    const mining = await mineBlock();
    console.log("Block mined:", mining.data);

    // 5. Validate blockchain
    const validation = await validateBlockchain();
    console.log("Validation result:", validation.data);

    return {
      success: true,
      results: { stats, cert, transfer, mining, validation },
    };
  } catch (error) {
    return {
      success: false,
      error: error.message,
    };
  }
};

// Export the base URL for components that need it
export { API_BASE };
export default api;

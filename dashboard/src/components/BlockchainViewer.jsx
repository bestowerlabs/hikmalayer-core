import React, { useEffect, useMemo, useState } from "react";
import {
  getExplorerOverview,
  getExplorerBlocks,
  getExplorerBlockByHash,
  getExplorerBlockByIndex,
  getPendingTransactionsStructured,
  searchExplorer,
} from "../api";

const PAGE_SIZE = 10;

const BlockchainViewer = ({ refreshTrigger }) => {
  const [overview, setOverview] = useState(null);
  const [blocks, setBlocks] = useState([]);
  const [totalBlocks, setTotalBlocks] = useState(0);
  const [pendingTxs, setPendingTxs] = useState([]);
  const [selectedBlock, setSelectedBlock] = useState(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResult, setSearchResult] = useState(null);
  const [offset, setOffset] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const totalPages = useMemo(
    () => Math.max(1, Math.ceil((totalBlocks || 0) / PAGE_SIZE)),
    [totalBlocks]
  );
  const currentPage = useMemo(() => Math.floor(offset / PAGE_SIZE) + 1, [offset]);

  const fetchExplorer = async (nextOffset = offset) => {
    setLoading(true);
    setError("");
    try {
      const [overviewRes, blocksRes, pendingRes] = await Promise.all([
        getExplorerOverview(),
        getExplorerBlocks({ offset: nextOffset, limit: PAGE_SIZE }),
        getPendingTransactionsStructured(),
      ]);

      setOverview(overviewRes.data);
      setBlocks(blocksRes.data.blocks || []);
      setTotalBlocks(blocksRes.data.total || 0);
      setPendingTxs(pendingRes.data || []);
      setOffset(nextOffset);
    } catch (err) {
      console.error(err);
      setError(err?.response?.data?.message || "Failed to load explorer data");
    } finally {
      setLoading(false);
    }
  };

  const openBlock = async (block) => {
    try {
      const res = await getExplorerBlockByIndex(block.index);
      setSelectedBlock(res.data || null);
    } catch (err) {
      console.error(err);
      setError("Failed to load block details");
    }
  };

  const runSearch = async () => {
    const trimmed = searchQuery.trim();
    if (!trimmed) {
      setSearchResult(null);
      return;
    }

    try {
      let result = null;

      if (/^\d+$/.test(trimmed)) {
        const byIndex = await getExplorerBlockByIndex(Number(trimmed));
        if (byIndex.data) {
          result = {
            query: trimmed,
            block_by_index: byIndex.data,
            block_by_hash: null,
            pending_matches: [],
          };
        }
      }

      if (!result && /^[a-fA-F0-9]{8,}$/.test(trimmed)) {
        const byHash = await getExplorerBlockByHash(trimmed);
        if (byHash.data) {
          result = {
            query: trimmed,
            block_by_index: null,
            block_by_hash: byHash.data,
            pending_matches: [],
          };
        }
      }

      if (!result) {
        const fallback = await searchExplorer(trimmed);
        result = fallback.data;
      }

      setSearchResult(result);
    } catch (err) {
      console.error(err);
      setError("Search failed");
    }
  };

  useEffect(() => {
    fetchExplorer(0);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshTrigger]);

  return (
    <div className="relative overflow-hidden rounded-2xl backdrop-blur-xl bg-white/10 border border-white/20 p-6">
      <div className="absolute inset-0 bg-gradient-to-br from-indigo-500/10 to-blue-500/10" />

      <div className="relative z-10 space-y-5">
        <div className="flex flex-col md:flex-row md:items-center md:justify-between gap-3">
          <h2 className="text-xl font-bold text-white">🚀 Hikmalayer Explorer (Advanced)</h2>
          <button
            className="bg-gradient-to-r from-indigo-500 to-blue-600 text-white px-4 py-2 rounded-xl font-medium disabled:opacity-50"
            onClick={() => fetchExplorer(offset)}
            disabled={loading}
          >
            {loading ? "Loading..." : "Refresh"}
          </button>
        </div>

        {error && (
          <div className="bg-red-500/20 border border-red-500/40 text-red-200 px-4 py-2 rounded-xl">
            {error}
          </div>
        )}

        {overview && (
          <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
            <MetricCard title="Blocks" value={overview.total_blocks} />
            <MetricCard title="Finalized" value={overview.finalized_height} />
            <MetricCard title="Pending TX" value={overview.pending_transactions} />
            <MetricCard title="Validators" value={overview.validators} />
            <MetricCard title="Peers" value={overview.peers} />
            <MetricCard title="Difficulty" value={overview.difficulty} />
            <MetricCard title="Chain Valid" value={overview.chain_valid ? "Yes" : "No"} />
            <MetricCard title="Latest Hash" value={truncate(overview.latest_hash, 18)} mono />
          </div>
        )}

        <div className="bg-white/5 border border-white/20 rounded-xl p-4">
          <h3 className="text-white font-semibold mb-2">Search (Block index / hash / tx id / address)</h3>
          <div className="flex flex-col md:flex-row gap-2">
            <input
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && runSearch()}
              placeholder="e.g. 12 or 0000ab..."
              className="flex-1 bg-black/20 text-white border border-white/20 rounded-lg px-3 py-2 outline-none"
              maxLength={128}
            />
            <button
              className="bg-cyan-600 text-white px-4 py-2 rounded-lg"
              onClick={runSearch}
            >
              Search
            </button>
          </div>

          {searchResult && (
            <div className="mt-3 text-sm text-gray-200 space-y-2">
              <p>
                Result for <span className="font-mono text-cyan-300">{searchResult.query}</span>
              </p>
              {searchResult.block_by_hash && (
                <button
                  className="text-left w-full bg-green-500/20 border border-green-500/40 rounded-lg p-2"
                  onClick={() => setSelectedBlock(searchResult.block_by_hash)}
                >
                  Found block by hash: #{searchResult.block_by_hash.block.index}
                </button>
              )}
              {searchResult.block_by_index && (
                <button
                  className="text-left w-full bg-blue-500/20 border border-blue-500/40 rounded-lg p-2"
                  onClick={() => setSelectedBlock(searchResult.block_by_index)}
                >
                  Found block by index: #{searchResult.block_by_index.block.index}
                </button>
              )}
              {searchResult.pending_matches?.length > 0 && (
                <div className="bg-yellow-500/20 border border-yellow-500/40 rounded-lg p-2">
                  Pending matches: {searchResult.pending_matches.length}
                </div>
              )}
            </div>
          )}
        </div>

        <div className="bg-white/5 border border-white/20 rounded-xl p-4">
          <div className="flex items-center justify-between mb-3">
            <h3 className="text-lg font-semibold text-white">Latest Blocks</h3>
            <div className="text-xs text-gray-300">Page {currentPage} / {totalPages}</div>
          </div>

          <div className="space-y-2 max-h-96 overflow-y-auto pr-1">
            {blocks.map((block) => (
              <button
                key={block.hash}
                className="w-full text-left bg-slate-900/50 hover:bg-slate-800/70 border border-white/10 rounded-lg p-3 transition"
                onClick={() => openBlock(block)}
              >
                <div className="flex items-center justify-between">
                  <div className="text-white font-semibold">Block #{block.index}</div>
                  <div className="text-xs text-gray-400">{new Date(block.timestamp).toLocaleString()}</div>
                </div>
                <div className="mt-1 text-xs text-gray-300 font-mono">{truncate(block.hash, 24)}</div>
                <div className="mt-1 text-xs text-gray-300">
                  tx: {block.tx_count} • validator: {block.validator || "N/A"} • nonce: {block.nonce}
                </div>
              </button>
            ))}
            {blocks.length === 0 && <p className="text-gray-400">No blocks yet.</p>}
          </div>

          <div className="mt-3 flex items-center justify-between">
            <button
              className="bg-slate-700 text-white px-3 py-1 rounded disabled:opacity-40"
              disabled={offset === 0 || loading}
              onClick={() => fetchExplorer(Math.max(0, offset - PAGE_SIZE))}
            >
              Previous
            </button>
            <button
              className="bg-slate-700 text-white px-3 py-1 rounded disabled:opacity-40"
              disabled={offset + PAGE_SIZE >= totalBlocks || loading}
              onClick={() => fetchExplorer(offset + PAGE_SIZE)}
            >
              Next
            </button>
          </div>
        </div>

        <div className="bg-white/5 border border-white/20 rounded-xl p-4">
          <h3 className="text-lg font-semibold text-white mb-2">Pending Transactions</h3>
          <div className="max-h-52 overflow-y-auto space-y-2">
            {pendingTxs.map((tx) => (
              <div key={tx.id} className="bg-black/20 border border-white/10 rounded-lg p-2 text-xs text-gray-200">
                <div className="font-mono">{tx.id}</div>
                <div>from: {tx.from || "SYSTEM"} → to: {tx.to}</div>
                <div>amount: {tx.amount} • type: {tx.transaction_type}</div>
              </div>
            ))}
            {pendingTxs.length === 0 && <p className="text-gray-400">No pending transactions.</p>}
          </div>
        </div>
      </div>

      {selectedBlock && (
        <div className="fixed inset-0 bg-black/50 backdrop-blur-sm flex items-center justify-center z-50 p-4">
          <div className="bg-slate-900 border border-white/20 rounded-2xl max-w-3xl w-full max-h-[80vh] overflow-y-auto p-5">
            <div className="flex items-center justify-between mb-3">
              <h3 className="text-white font-bold text-lg">Block #{selectedBlock.block.index}</h3>
              <button className="text-gray-300 text-2xl" onClick={() => setSelectedBlock(null)}>
                ×
              </button>
            </div>
            <div className="text-sm text-gray-300 mb-2">PoW valid: {selectedBlock.pow_valid ? "Yes" : "No"}</div>
            <pre className="text-xs text-gray-200 bg-black/30 rounded-xl p-3 overflow-x-auto">
              {JSON.stringify(selectedBlock.block, null, 2)}
            </pre>
          </div>
        </div>
      )}
    </div>
  );
};

const MetricCard = ({ title, value, mono = false }) => (
  <div className="bg-white/10 border border-white/20 rounded-xl p-3">
    <div className="text-xs text-gray-300">{title}</div>
    <div className={`text-white font-semibold ${mono ? "font-mono text-sm" : "text-lg"}`}>
      {value}
    </div>
  </div>
);

const truncate = (value, size = 16) => {
  if (!value) return "N/A";
  return value.length > size ? `${value.slice(0, size)}...` : value;
};

export default BlockchainViewer;

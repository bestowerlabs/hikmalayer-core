import React, { useState, useEffect } from "react";
import { useAuthenticatedApi } from "../hooks/useAuthenticatedApi";

const API_BASE = "http://127.0.0.1:3000";

const TokenManager = ({ refreshTrigger, onTokenTransfer }) => {
  const [transferData, setTransferData] = useState({
    from: "",
    to: "",
    amount: 0,
    public_key: "",
    signature: "",
  });
  const [nextNonce, setNextNonce] = useState(null);
  const [balanceAddress, setBalanceAddress] = useState("");
  const [balance, setBalance] = useState(null);
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState("");
  const [userBalance, setUserBalance] = useState(null);

  const { authenticatedFetch, isConnected, account } = useAuthenticatedApi();

  useEffect(() => {
    // Auto-populate from field with connected wallet address
    if (account && !transferData.from) {
      setTransferData((prev) => ({
        ...prev,
        from: account,
      }));
      setBalanceAddress(account);
    }
  }, [account, transferData.from]);

  useEffect(() => {
    if (account) {
      fetchUserBalance();
    }
  }, [account, refreshTrigger]);

  const fetchUserBalance = async () => {
    if (!account) return;

    try {
      const response = await fetch(`${API_BASE}/tokens/balance/${account}`);
      if (response.ok) {
        const result = await response.json();
        setUserBalance(result.balance);
      }
    } catch (error) {
      console.error("Failed to fetch user balance:", error);
    }
  };

  const handleTransferInputChange = (e) => {
    const { name, value } = e.target;
    setTransferData((prev) => ({
      ...prev,
      [name]: name === "amount" ? parseInt(value) || 0 : value,
    }));
  };

  // The exact hikma-wallet command that produces the required signature.
  const signingCommand = () =>
    `hikma-wallet sign-transfer ${transferData.from || "<from>"} ${
      transferData.to || "<to>"
    } ${transferData.amount || 0} ${nextNonce ?? "<nonce>"} <private_key>`;

  const fetchNextNonce = async () => {
    if (!transferData.from) return null;
    const nonceResponse = await fetch(
      `${API_BASE}/tokens/nonce/${transferData.from}`
    );
    const { next_nonce } = await nonceResponse.json();
    setNextNonce(next_nonce);
    return next_nonce;
  };

  useEffect(() => {
    fetchNextNonce().catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [transferData.from, refreshTrigger]);

  const handleTransferTokens = async (e) => {
    e.preventDefault();

    if (!transferData.from || !transferData.to || transferData.amount <= 0) {
      setMessage("Please fill in all fields with valid values");
      return;
    }

    if (transferData.from === transferData.to) {
      setMessage("Cannot transfer tokens to the same address");
      return;
    }

    if (!transferData.public_key || !transferData.signature) {
      setMessage(
        `Transfers are signed offline with the Hikmalayer wallet. Run: ${signingCommand()}`
      );
      return;
    }

    setLoading(true);
    setMessage("Submitting signed transfer...");

    try {
      const nonce = nextNonce ?? (await fetchNextNonce());

      const response = await authenticatedFetch("/tokens/transfer", {
        method: "POST",
        body: JSON.stringify({
          from: transferData.from,
          to: transferData.to,
          amount: transferData.amount,
          nonce,
          public_key: transferData.public_key,
          signature: transferData.signature,
        }),
      });

      const result = await response.json();

      if (response.ok) {
        setMessage(
          `Successfully transferred ${transferData.amount} MCT from ${transferData.from} to ${transferData.to}`
        );

        // Reset form but keep from address
        setTransferData(() => ({
          from: account || "",
          to: "",
          amount: 0,
          public_key: "",
          signature: "",
        }));

        // Refresh user balance
        fetchUserBalance();

        if (onTokenTransfer) {
          onTokenTransfer();
        }
      } else {
        setMessage(`Transfer failed: ${result.message}`);
      }
    } catch (error) {
      setMessage(`Error transferring tokens: ${error.message}`);
    } finally {
      setLoading(false);
    }
  };

  const handleCheckBalance = async (e) => {
    e.preventDefault();

    if (!balanceAddress) {
      setMessage("Please enter an address to check balance");
      return;
    }

    setLoading(true);
    setMessage("Checking balance...");

    try {
      const response = await fetch(
        `${API_BASE}/tokens/balance/${balanceAddress}`
      );

      if (response.ok) {
        const result = await response.json();
        setBalance(result.balance);
        setMessage(`Balance for ${balanceAddress}: ${result.balance} MCT`);
      } else {
        setMessage("Failed to fetch balance");
      }
    } catch (error) {
      setMessage(`Error checking balance: ${error.message}`);
    } finally {
      setLoading(false);
    }
  };

  const maxTransferAmount = () => {
    if (userBalance && transferData.from === account) {
      setTransferData((prev) => ({
        ...prev,
        amount: userBalance,
      }));
    }
  };

  return (
    <div className="relative overflow-hidden rounded-2xl backdrop-blur-xl bg-white/10 border border-white/20 p-6">
      <div className="absolute inset-0 bg-gradient-to-br from-yellow-500/10 to-orange-500/10"></div>

      <div className="relative z-10">
        <div className="flex items-center mb-6">
          <div className="p-2 rounded-xl bg-gradient-to-r from-yellow-500/20 to-orange-500/20 mr-3">
            <span className="text-2xl">💰</span>
          </div>
          <div>
            <h2 className="text-xl font-bold text-white">Token Manager</h2>
            <p className="text-sm text-gray-300">Metacation Token (MCT)</p>
          </div>
        </div>

        {!isConnected && (
          <div className="mb-6 p-4 rounded-xl bg-yellow-500/20 text-yellow-300 border border-yellow-500/30">
            <div className="flex items-center gap-2">
              <span className="text-lg">⚠️</span>
              <span className="font-medium">
                Please connect your wallet to manage tokens
              </span>
            </div>
          </div>
        )}

        {/* User Balance Display */}
        {isConnected && userBalance !== null && (
          <div className="mb-6 p-4 rounded-xl bg-gradient-to-r from-green-500/20 to-emerald-500/20 border border-green-500/30 backdrop-blur-sm">
            <div className="flex items-center justify-between">
              <div>
                <div className="text-sm text-green-300">Your MCT Balance</div>
                <div className="text-2xl font-bold text-white">
                  {userBalance} MCT
                </div>
              </div>
              <div className="text-4xl">💎</div>
            </div>
          </div>
        )}

        {/* Status Message */}
        {message && (
          <div
            className={`mb-6 p-4 rounded-xl backdrop-blur-sm border transition-all duration-500 ${
              message.includes("Successfully") ||
              message.includes("Balance for")
                ? "bg-green-500/20 text-green-300 border-green-500/30"
                : message.includes("Transferring") ||
                  message.includes("Checking")
                ? "bg-blue-500/20 text-blue-300 border-blue-500/30 animate-pulse"
                : "bg-red-500/20 text-red-300 border-red-500/30"
            }`}
          >
            <div className="flex items-center gap-2">
              <span className="text-lg">
                {message.includes("Successfully") ||
                message.includes("Balance for")
                  ? "✅"
                  : message.includes("Transferring") ||
                    message.includes("Checking")
                  ? "⏳"
                  : "❌"}
              </span>
              <span className="font-medium">{message}</span>
            </div>
          </div>
        )}

        <div className="space-y-6">
          {/* Transfer Tokens Form */}
          <form
            onSubmit={handleTransferTokens}
            className="p-6 rounded-xl bg-gradient-to-r from-indigo-500/20 to-purple-500/20 border border-indigo-500/30 backdrop-blur-sm"
          >
            <h3 className="text-lg font-semibold text-white mb-4 flex items-center gap-2">
              <span>💸</span>
              <span>Transfer Tokens</span>
            </h3>

            <div className="space-y-4">
              <div>
                <label className="block text-sm font-medium text-gray-200 mb-2">
                  From Address
                </label>
                <input
                  type="text"
                  name="from"
                  value={transferData.from}
                  onChange={handleTransferInputChange}
                  placeholder="Sender address"
                  className="w-full bg-white/10 border border-white/20 rounded-lg px-3 py-2 text-white placeholder-gray-400 backdrop-blur-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
                  disabled={loading || !isConnected}
                />
                {account && transferData.from === account && (
                  <p className="text-xs text-cyan-300 mt-1">
                    💡 Sending from your connected wallet
                  </p>
                )}
              </div>

              <div>
                <label className="block text-sm font-medium text-gray-200 mb-2">
                  To Address
                </label>
                <input
                  type="text"
                  name="to"
                  value={transferData.to}
                  onChange={handleTransferInputChange}
                  placeholder="Recipient address"
                  className="w-full bg-white/10 border border-white/20 rounded-lg px-3 py-2 text-white placeholder-gray-400 backdrop-blur-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
                  disabled={loading || !isConnected}
                />
              </div>

              <div>
                <label className="block text-sm font-medium text-gray-200 mb-2">
                  Amount (MCT)
                </label>
                <div className="flex gap-2">
                  <input
                    type="number"
                    name="amount"
                    value={transferData.amount}
                    onChange={handleTransferInputChange}
                    placeholder="Token amount"
                    min="1"
                    className="flex-1 bg-white/10 border border-white/20 rounded-lg px-3 py-2 text-white placeholder-gray-400 backdrop-blur-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
                    disabled={loading || !isConnected}
                  />
                  {userBalance && transferData.from === account && (
                    <button
                      type="button"
                      onClick={maxTransferAmount}
                      className="px-4 py-2 bg-gradient-to-r from-cyan-500 to-blue-600 text-white rounded-lg font-medium hover:scale-105 transition-transform disabled:opacity-50"
                      disabled={loading || !isConnected}
                    >
                      Max
                    </button>
                  )}
                </div>
                {userBalance && transferData.from === account && (
                  <p className="text-xs text-gray-400 mt-1">
                    Available: {userBalance} MCT
                  </p>
                )}
              </div>

              {/* Native offline signing: keys never leave the wallet */}
              <div className="p-3 rounded-lg bg-black/30 border border-white/10">
                <p className="text-xs text-gray-300 mb-2">
                  🔏 Sign this transfer offline with the Hikmalayer wallet
                  (next nonce: <strong>{nextNonce ?? "…"}</strong>):
                </p>
                <code className="block text-[11px] text-cyan-300 break-all mb-3">
                  {signingCommand()}
                </code>
                <label className="block text-xs font-medium text-gray-200 mb-1">
                  Public Key (hex)
                </label>
                <input
                  type="text"
                  name="public_key"
                  value={transferData.public_key}
                  onChange={handleTransferInputChange}
                  placeholder="Uncompressed secp256k1 public key"
                  className="w-full bg-white/10 border border-white/20 rounded-lg px-3 py-2 text-white placeholder-gray-400 text-xs mb-2"
                  disabled={loading}
                />
                <label className="block text-xs font-medium text-gray-200 mb-1">
                  Signature (hex)
                </label>
                <input
                  type="text"
                  name="signature"
                  value={transferData.signature}
                  onChange={handleTransferInputChange}
                  placeholder="hikma-wallet signature output"
                  className="w-full bg-white/10 border border-white/20 rounded-lg px-3 py-2 text-white placeholder-gray-400 text-xs"
                  disabled={loading}
                />
              </div>
            </div>

            <button
              type="submit"
              disabled={
                loading ||
                !transferData.from ||
                !transferData.to ||
                transferData.amount <= 0 ||
                !transferData.public_key ||
                !transferData.signature
              }
              className="relative overflow-hidden mt-6 w-full bg-gradient-to-r from-indigo-500 to-purple-600 text-white px-6 py-3 rounded-xl font-medium transition-all duration-300 hover:scale-105 hover:shadow-lg hover:shadow-indigo-500/25 disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center gap-2 group"
            >
              <div className="absolute inset-0 bg-gradient-to-r from-indigo-600 to-purple-700 opacity-0 group-hover:opacity-100 transition-opacity duration-300"></div>
              <span className="relative z-10 text-xl">
                {loading ? "⏳" : "💸"}
              </span>
              <span className="relative z-10">
                {loading ? "Transferring..." : "Submit Signed Transfer"}
              </span>
            </button>
          </form>

          {/* Check Balance Form */}
          <form
            onSubmit={handleCheckBalance}
            className="p-6 rounded-xl bg-gradient-to-r from-green-500/20 to-teal-500/20 border border-green-500/30 backdrop-blur-sm"
          >
            <h3 className="text-lg font-semibold text-white mb-4 flex items-center gap-2">
              <span>📊</span>
              <span>Check Token Balance</span>
            </h3>

            <div className="mb-4">
              <label className="block text-sm font-medium text-gray-200 mb-2">
                Address
              </label>
              <div className="flex gap-2">
                <input
                  type="text"
                  value={balanceAddress}
                  onChange={(e) => setBalanceAddress(e.target.value)}
                  placeholder="Enter address to check balance"
                  className="flex-1 bg-white/10 border border-white/20 rounded-lg px-3 py-2 text-white placeholder-gray-400 backdrop-blur-sm focus:outline-none focus:ring-2 focus:ring-green-500 focus:border-transparent"
                  disabled={loading}
                />
                {account && (
                  <button
                    type="button"
                    onClick={() => setBalanceAddress(account)}
                    className="px-4 py-2 bg-gradient-to-r from-cyan-500 to-blue-600 text-white rounded-lg font-medium hover:scale-105 transition-transform"
                    disabled={loading}
                  >
                    My Address
                  </button>
                )}
              </div>
            </div>

            {balance !== null && (
              <div className="mb-4 p-3 rounded-lg bg-white/10 border border-white/20 backdrop-blur-sm">
                <div className="text-sm text-gray-300 mb-1">
                  Balance Result:
                </div>
                <div className="text-xl font-bold text-white">
                  {balance} MCT
                </div>
              </div>
            )}

            <button
              type="submit"
              disabled={loading || !balanceAddress}
              className="relative overflow-hidden w-full bg-gradient-to-r from-green-500 to-teal-600 text-white px-6 py-3 rounded-xl font-medium transition-all duration-300 hover:scale-105 hover:shadow-lg hover:shadow-green-500/25 disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center gap-2 group"
            >
              <div className="absolute inset-0 bg-gradient-to-r from-green-600 to-teal-700 opacity-0 group-hover:opacity-100 transition-opacity duration-300"></div>
              <span className="relative z-10 text-xl">
                {loading ? "⏳" : "📊"}
              </span>
              <span className="relative z-10">
                {loading ? "Checking Balance..." : "Check Balance"}
              </span>
            </button>
          </form>

          {/* Token Information */}
          <div className="p-6 rounded-xl bg-gradient-to-r from-gray-500/20 to-slate-500/20 border border-gray-500/30 backdrop-blur-sm">
            <h3 className="text-lg font-semibold text-white mb-4 flex items-center gap-2">
              <span>ℹ️</span>
              <span>Token Information</span>
            </h3>

            <div className="space-y-3 text-sm">
              <div className="flex justify-between">
                <span className="text-gray-300">Token Name:</span>
                <span className="text-white font-medium">Metacation Token</span>
              </div>
              <div className="flex justify-between">
                <span className="text-gray-300">Symbol:</span>
                <span className="text-white font-medium">MCT</span>
              </div>
              <div className="flex justify-between">
                <span className="text-gray-300">Initial Supply:</span>
                <span className="text-white font-medium">1,000 MCT</span>
              </div>
              <div className="flex justify-between">
                <span className="text-gray-300">Admin Address:</span>
                <span className="text-white font-mono text-xs">admin</span>
              </div>
            </div>

            <div className="mt-4 p-3 rounded-lg bg-blue-500/20 border border-blue-500/30">
              <div className="text-xs text-blue-300">
                💡 <strong>How it works:</strong> Transfer tokens between
                addresses on HIKMAON. All transfers are recorded as transactions
                and must be mined into blocks to be confirmed.
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

export default TokenManager;

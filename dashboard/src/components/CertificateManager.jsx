import React, { useState, useEffect } from "react";
import { useAuthenticatedApi } from "../hooks/useAuthenticatedApi";

const CertificateManager = ({ onUpdate }) => {
  const [formData, setFormData] = useState({
    id: "",
    issued_to: "",
    description: "",
  });
  const [verifyId, setVerifyId] = useState("");
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState("");
  const [certificates, setCertificates] = useState([]);

  const { authenticatedFetch, isConnected, account } = useAuthenticatedApi();

  useEffect(() => {
    // Auto-populate issued_to with connected wallet address
    if (account && !formData.issued_to) {
      setFormData((prev) => ({
        ...prev,
        issued_to: account,
      }));
    }
  }, [account, formData.issued_to]);

  const handleInputChange = (e) => {
    const { name, value } = e.target;
    setFormData((prev) => ({
      ...prev,
      [name]: value,
    }));
  };

  const generateCertificateId = () => {
    const timestamp = Date.now();
    const random = Math.random().toString(36).substring(2, 8);
    return `CERT_${timestamp}_${random}`.toUpperCase();
  };

  const handleIssueCertificate = async (e) => {
    e.preventDefault();

    if (!isConnected) {
      setMessage("Please connect your wallet first");
      return;
    }

    if (!formData.id || !formData.issued_to || !formData.description) {
      setMessage("Please fill in all fields");
      return;
    }

    setLoading(true);
    setMessage("Issuing certificate...");

    try {
      const response = await authenticatedFetch("/certificates/issue", {
        method: "POST",
        body: JSON.stringify(formData),
      });

      const result = await response.json();

      if (response.ok) {
        setMessage(`Certificate successfully issued: ${formData.id}`);

        // Add to local certificates list
        const newCert = {
          ...formData,
          issued_at: new Date().toISOString(),
          status: "pending_mining",
        };
        setCertificates((prev) => [newCert, ...prev]);

        // Reset form but keep issued_to as current account
        setFormData({
          id: "",
          issued_to: account || "",
          description: "",
        });

        if (onUpdate) {
          onUpdate();
        }
      } else {
        setMessage(`Failed to issue certificate: ${result.message}`);
      }
    } catch (error) {
      setMessage(`Error issuing certificate: ${error.message}`);
    } finally {
      setLoading(false);
    }
  };

  const handleVerifyCertificate = async (e) => {
    e.preventDefault();

    if (!isConnected) {
      setMessage("Please connect your wallet first");
      return;
    }

    if (!verifyId) {
      setMessage("Please enter a certificate ID to verify");
      return;
    }

    setLoading(true);
    setMessage("Verifying certificate...");

    try {
      const response = await authenticatedFetch("/certificates/verify", {
        method: "POST",
        body: JSON.stringify({ id: verifyId }),
      });

      const result = await response.json();

      if (response.ok) {
        setMessage(`Certificate verification: ${result.message}`);
      } else {
        setMessage(`Verification failed: ${result.message}`);
      }
    } catch (error) {
      setMessage(`Error verifying certificate: ${error.message}`);
    } finally {
      setLoading(false);
      setVerifyId("");
    }
  };

  return (
    <div className="relative overflow-hidden rounded-2xl backdrop-blur-xl bg-white/10 border border-white/20 p-6">
      <div className="absolute inset-0 bg-gradient-to-br from-purple-500/10 to-pink-500/10"></div>

      <div className="relative z-10">
        <div className="flex items-center mb-6">
          <div className="p-2 rounded-xl bg-gradient-to-r from-purple-500/20 to-pink-500/20 mr-3">
            <span className="text-2xl">📜</span>
          </div>
          <h2 className="text-xl font-bold text-white">Certificate Manager</h2>
        </div>

        {!isConnected && (
          <div className="mb-6 p-4 rounded-xl bg-yellow-500/20 text-yellow-300 border border-yellow-500/30">
            <div className="flex items-center gap-2">
              <span className="text-lg">⚠️</span>
              <span className="font-medium">
                Please connect your wallet to manage certificates
              </span>
            </div>
          </div>
        )}

        {/* Status Message */}
        {message && (
          <div
            className={`mb-6 p-4 rounded-xl backdrop-blur-sm border transition-all duration-500 ${
              message.includes("successfully") || message.includes("verified")
                ? "bg-green-500/20 text-green-300 border-green-500/30"
                : message.includes("Issuing") || message.includes("Verifying")
                ? "bg-blue-500/20 text-blue-300 border-blue-500/30 animate-pulse"
                : "bg-red-500/20 text-red-300 border-red-500/30"
            }`}
          >
            <div className="flex items-center gap-2">
              <span className="text-lg">
                {message.includes("successfully") ||
                message.includes("verified")
                  ? "✅"
                  : message.includes("Issuing") || message.includes("Verifying")
                  ? "⏳"
                  : "❌"}
              </span>
              <span className="font-medium">{message}</span>
            </div>
          </div>
        )}

        <div className="space-y-6">
          {/* Issue Certificate Form */}
          <form
            onSubmit={handleIssueCertificate}
            className="p-6 rounded-xl bg-gradient-to-r from-indigo-500/20 to-purple-500/20 border border-indigo-500/30 backdrop-blur-sm"
          >
            <h3 className="text-lg font-semibold text-white mb-4 flex items-center gap-2">
              <span>✨</span>
              <span>Issue New Certificate</span>
            </h3>

            <div className="space-y-4">
              <div className="flex gap-4">
                <div className="flex-1">
                  <label className="block text-sm font-medium text-gray-200 mb-2">
                    Certificate ID
                  </label>
                  <div className="flex gap-2">
                    <input
                      type="text"
                      name="id"
                      value={formData.id}
                      onChange={handleInputChange}
                      placeholder="Enter certificate ID"
                      className="flex-1 bg-white/10 border border-white/20 rounded-lg px-3 py-2 text-white placeholder-gray-400 backdrop-blur-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
                      disabled={loading || !isConnected}
                    />
                    <button
                      type="button"
                      onClick={() =>
                        setFormData((prev) => ({
                          ...prev,
                          id: generateCertificateId(),
                        }))
                      }
                      className="px-4 py-2 bg-gradient-to-r from-cyan-500 to-blue-600 text-white rounded-lg font-medium hover:scale-105 transition-transform disabled:opacity-50"
                      disabled={loading || !isConnected}
                    >
                      Generate
                    </button>
                  </div>
                </div>
              </div>

              <div>
                <label className="block text-sm font-medium text-gray-200 mb-2">
                  Issued To (Address)
                </label>
                <input
                  type="text"
                  name="issued_to"
                  value={formData.issued_to}
                  onChange={handleInputChange}
                  placeholder="Recipient address"
                  className="w-full bg-white/10 border border-white/20 rounded-lg px-3 py-2 text-white placeholder-gray-400 backdrop-blur-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent"
                  disabled={loading || !isConnected}
                />
                {account && formData.issued_to === account && (
                  <p className="text-xs text-cyan-300 mt-1">
                    💡 Issuing to your connected wallet address
                  </p>
                )}
              </div>

              <div>
                <label className="block text-sm font-medium text-gray-200 mb-2">
                  Description
                </label>
                <textarea
                  name="description"
                  value={formData.description}
                  onChange={handleInputChange}
                  placeholder="Certificate description"
                  rows={3}
                  className="w-full bg-white/10 border border-white/20 rounded-lg px-3 py-2 text-white placeholder-gray-400 backdrop-blur-sm focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent resize-none"
                  disabled={loading || !isConnected}
                />
              </div>
            </div>

            <button
              type="submit"
              disabled={
                loading ||
                !isConnected ||
                !formData.id ||
                !formData.issued_to ||
                !formData.description
              }
              className="relative overflow-hidden mt-6 w-full bg-gradient-to-r from-indigo-500 to-purple-600 text-white px-6 py-3 rounded-xl font-medium transition-all duration-300 hover:scale-105 hover:shadow-lg hover:shadow-indigo-500/25 disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center gap-2 group"
            >
              <div className="absolute inset-0 bg-gradient-to-r from-indigo-600 to-purple-700 opacity-0 group-hover:opacity-100 transition-opacity duration-300"></div>
              <span className="relative z-10 text-xl">
                {loading ? "⏳" : "📜"}
              </span>
              <span className="relative z-10">
                {loading
                  ? "Issuing Certificate..."
                  : !isConnected
                  ? "Connect Wallet to Issue"
                  : "Issue Certificate"}
              </span>
            </button>
          </form>

          {/* Verify Certificate Form */}
          <form
            onSubmit={handleVerifyCertificate}
            className="p-6 rounded-xl bg-gradient-to-r from-green-500/20 to-teal-500/20 border border-green-500/30 backdrop-blur-sm"
          >
            <h3 className="text-lg font-semibold text-white mb-4 flex items-center gap-2">
              <span>✅</span>
              <span>Verify Certificate</span>
            </h3>

            <div className="mb-4">
              <label className="block text-sm font-medium text-gray-200 mb-2">
                Certificate ID
              </label>
              <input
                type="text"
                value={verifyId}
                onChange={(e) => setVerifyId(e.target.value)}
                placeholder="Enter certificate ID to verify"
                className="w-full bg-white/10 border border-white/20 rounded-lg px-3 py-2 text-white placeholder-gray-400 backdrop-blur-sm focus:outline-none focus:ring-2 focus:ring-green-500 focus:border-transparent"
                disabled={loading || !isConnected}
              />
            </div>

            <button
              type="submit"
              disabled={loading || !isConnected || !verifyId}
              className="relative overflow-hidden w-full bg-gradient-to-r from-green-500 to-teal-600 text-white px-6 py-3 rounded-xl font-medium transition-all duration-300 hover:scale-105 hover:shadow-lg hover:shadow-green-500/25 disabled:opacity-50 disabled:cursor-not-allowed flex items-center justify-center gap-2 group"
            >
              <div className="absolute inset-0 bg-gradient-to-r from-green-600 to-teal-700 opacity-0 group-hover:opacity-100 transition-opacity duration-300"></div>
              <span className="relative z-10 text-xl">
                {loading ? "⏳" : "✅"}
              </span>
              <span className="relative z-10">
                {loading
                  ? "Verifying..."
                  : !isConnected
                  ? "Connect Wallet to Verify"
                  : "Verify Certificate"}
              </span>
            </button>
          </form>

          {/* Recent Certificates */}
          {certificates.length > 0 && (
            <div className="p-6 rounded-xl bg-gradient-to-r from-gray-500/20 to-slate-500/20 border border-gray-500/30 backdrop-blur-sm">
              <h3 className="text-lg font-semibold text-white mb-4 flex items-center gap-2">
                <span>📋</span>
                <span>Recent Certificates</span>
              </h3>

              <div className="space-y-3 max-h-60 overflow-y-auto">
                {certificates.map((cert, index) => (
                  <div
                    key={index}
                    className="p-3 rounded-lg bg-white/10 border border-white/20 backdrop-blur-sm"
                  >
                    <div className="flex justify-between items-start mb-2">
                      <div className="font-mono text-sm text-cyan-300">
                        {cert.id}
                      </div>
                      <span
                        className={`px-2 py-1 rounded text-xs ${
                          cert.status === "pending_mining"
                            ? "bg-yellow-500/20 text-yellow-300 border border-yellow-500/30"
                            : "bg-green-500/20 text-green-300 border border-green-500/30"
                        }`}
                      >
                        {cert.status === "pending_mining"
                          ? "Pending Mining"
                          : "Confirmed"}
                      </span>
                    </div>
                    <div className="text-sm text-gray-300 truncate mb-1">
                      To: {cert.issued_to}
                    </div>
                    <div className="text-xs text-gray-400 truncate">
                      {cert.description}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

export default CertificateManager;

const AuthRequired = ({ action }) => (
  <div className="group relative overflow-hidden rounded-2xl backdrop-blur-xl bg-white/10 border border-white/20 p-6">
    <div className="absolute inset-0 bg-gradient-to-br from-yellow-500/10 to-orange-500/10"></div>
    <div className="relative z-10 text-center">
      <div className="text-4xl mb-4">🔒</div>
      <h3 className="text-xl font-bold text-white mb-2">
        Authentication Required
      </h3>
      <p className="text-gray-300">Connect your wallet to {action}</p>
    </div>
  </div>
);

export default AuthRequired;


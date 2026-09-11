import { Outlet, Link, useLocation } from "react-router-dom";

const NAV_ITEMS = [
  { path: "/", label: "Dashboard" },
  { path: "/wallet", label: "Wallet" },
  { path: "/explorer", label: "Explorer" },
  { path: "/dex", label: "DEX" },
  { path: "/staking", label: "Staking" },
  { path: "/certificates", label: "Certificates" },
  { path: "/credentials", label: "Credentials" },
  { path: "/vesting", label: "Vesting" },
  { path: "/governance", label: "Governance" },
  { path: "/mining", label: "Mining" },
];

const Layout = () => {
  const location = useLocation();

  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-900 via-purple-900 to-slate-900 relative overflow-hidden">
      <div className="absolute inset-0 overflow-hidden">
        <div className="absolute -top-40 -right-40 w-80 h-80 bg-blue-500/10 rounded-full blur-3xl animate-pulse"></div>
        <div
          className="absolute -bottom-40 -left-40 w-80 h-80 bg-purple-500/10 rounded-full blur-3xl animate-pulse"
          style={{ animationDelay: "1000ms" }}
        ></div>
      </div>

      <nav className="relative z-20 border-b border-white/10 backdrop-blur-sm">
        <div className="max-w-7xl mx-auto px-4 md:px-8 flex flex-wrap gap-1 py-3">
          {NAV_ITEMS.map((item) => {
            const active = location.pathname === item.path;
            return (
              <Link
                key={item.path}
                to={item.path}
                className={`px-3 py-1.5 rounded-lg text-sm font-medium transition-colors ${
                  active
                    ? "bg-blue-500/30 text-white"
                    : "text-gray-300 hover:bg-white/10 hover:text-white"
                }`}
              >
                {item.label}
              </Link>
            );
          })}
        </div>
      </nav>

      <div className="relative z-10 p-4 md:p-8">
        <div className="max-w-7xl mx-auto space-y-8">
          <Outlet />
        </div>
      </div>
    </div>
  );
};

export default Layout;

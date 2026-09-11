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
    <div className="min-h-screen bg-hikma-bg font-body text-hikma-paper relative overflow-hidden">
      {/* Subtle ember glow, echoing the site's hero without competing with it */}
      <div className="absolute inset-0 overflow-hidden pointer-events-none">
        <div className="absolute -top-40 -right-40 w-96 h-96 bg-hikma-ember/5 rounded-full blur-3xl"></div>
        <div className="absolute -bottom-40 -left-40 w-96 h-96 bg-hikma-ember/5 rounded-full blur-3xl"></div>
      </div>

      <header className="relative z-20 border-b border-hikma-rule">
        <div className="max-w-7xl mx-auto px-4 md:px-8 py-4 flex flex-wrap items-center gap-6">
          <Link to="/" className="flex items-center gap-2.5 group">
            <img src="/hikma-logo.png" alt="" className="w-8 h-8" />
            <span className="font-display font-bold text-xl tracking-wide uppercase text-hikma-paper group-hover:text-hikma-ember transition-colors">
              Hikmalayer
            </span>
          </Link>

          <nav className="flex flex-wrap gap-1 ml-auto">
            {NAV_ITEMS.map((item) => {
              const active = location.pathname === item.path;
              return (
                <Link
                  key={item.path}
                  to={item.path}
                  className={`px-3 py-1.5 rounded text-xs font-medium uppercase tracking-wide transition-colors ${
                    active
                      ? "bg-hikma-ember/20 text-hikma-ember border border-hikma-ember/40"
                      : "text-hikma-muted hover:text-hikma-paper hover:bg-white/5"
                  }`}
                >
                  {item.label}
                </Link>
              );
            })}
          </nav>
        </div>
      </header>

      <main className="relative z-10 px-4 md:px-8 py-8">
        <div className="max-w-7xl mx-auto space-y-8">
          <Outlet />
        </div>
      </main>
    </div>
  );
};

export default Layout;

import React, { useEffect, useState } from "react";

// Animated Counter Component
const AnimatedCounter = ({ value, duration = 2000 }) => {
  const [count, setCount] = useState(0);

  useEffect(() => {
    let startTime = null;
    const startValue = count;
    const endValue = value;

    const animate = (timestamp) => {
      if (!startTime) startTime = timestamp;
      const progress = Math.min((timestamp - startTime) / duration, 1);
      const easeOutCubic = 1 - Math.pow(1 - progress, 3);
      setCount(Math.floor(startValue + (endValue - startValue) * easeOutCubic));

      if (progress < 1) {
        requestAnimationFrame(animate);
      }
    };

    if (value !== count) {
      requestAnimationFrame(animate);
    }
  }, [value]);

  return <span>{count}</span>;
};

const StatsCard = ({ title, value, icon, color, subtitle, index }) => (
  <div
    className={`group relative overflow-hidden rounded-2xl backdrop-blur-xl bg-white/10 border border-white/20 p-6 hover:bg-white/20 transition-all duration-500 hover:scale-105 hover:shadow-2xl hover:shadow-${
      color.split("-")[1]
    }-500/20`}
    style={{
      animationDelay: `${index * 100}ms`,
      background:
        "linear-gradient(135deg, rgba(255,255,255,0.1), rgba(255,255,255,0.05))",
    }}
  >
    <div className="absolute inset-0 bg-gradient-to-br from-white/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity duration-500"></div>
    <div className="relative z-10 flex items-center justify-between">
      <div>
        <p className="text-sm font-medium text-gray-300 mb-2">{title}</p>
        <p
          className={`text-3xl font-bold ${color} transition-all duration-300 group-hover:scale-110`}
        >
          {/* Only numbers animate; textual statuses (e.g. "Valid") render
              as-is — animating them printed a stray leading 0. */}
          {typeof value === "number" ? <AnimatedCounter value={value} /> : value}
        </p>
        {subtitle && (
          <p className="text-xs text-gray-400 mt-2 opacity-70 group-hover:opacity-100 transition-opacity duration-300">
            {subtitle}
          </p>
        )}
      </div>
      <div
        className={`text-4xl ${color} transform transition-all duration-300 group-hover:scale-125 group-hover:rotate-12`}
      >
        {icon}
      </div>
    </div>
    <div className="absolute bottom-0 left-0 w-full h-1 bg-gradient-to-r from-transparent via-white/30 to-transparent transform scale-x-0 group-hover:scale-x-100 transition-transform duration-500"></div>
  </div>
);

const StatsGrid = ({ stats }) => {
  return (
    <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6 mb-8">
      <StatsCard
        title="Total Blocks"
        value={stats.total_blocks}
        icon="📦"
        color="text-blue-400"
        subtitle="Including genesis block"
        index={0}
      />
      <StatsCard
        title="Pending Transactions"
        value={stats.pending_transactions}
        icon="⏳"
        color="text-orange-400"
        subtitle="Waiting to be mined"
        index={1}
      />
      <StatsCard
        title="Mining Difficulty"
        value={stats.difficulty}
        icon="⚡"
        color="text-yellow-400"
        subtitle="Proof-of-work complexity"
        index={2}
      />
      <StatsCard
        title="Chain Status"
        value={stats.is_valid ? "Valid" : "Invalid"}
        icon={stats.is_valid ? "✅" : "❌"}
        color={stats.is_valid ? "text-green-400" : "text-red-400"}
        subtitle={
          stats.latest_hash ? `Hash: ${stats.latest_hash.slice(0, 12)}...` : ""
        }
        index={3}
      />
    </div>
  );
};

export default StatsGrid;

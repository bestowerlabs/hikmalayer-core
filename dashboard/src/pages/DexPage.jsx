import DexSwap from "../components/DexSwap";
import DexLiquidity from "../components/DexLiquidity";

const DexPage = ({ refreshTrigger, onUpdate }) => (
  <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
    <DexSwap refreshTrigger={refreshTrigger} onUpdate={onUpdate} />
    <DexLiquidity refreshTrigger={refreshTrigger} onUpdate={onUpdate} />
  </div>
);

export default DexPage;

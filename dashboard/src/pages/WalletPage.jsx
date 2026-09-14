import ProtectedAction from "../components/ProtectedAction";
import AuthRequired from "../components/AuthRequired";
import WalletPanel from "../components/WalletPanel";
import TokenManager from "../components/TokenManager";

const WalletPage = ({ refreshTrigger, onUpdate }) => (
  <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
    <WalletPanel refreshTrigger={refreshTrigger} />
    <ProtectedAction fallback={<AuthRequired action="transfer tokens" />}>
      <TokenManager onUpdate={onUpdate} refreshTrigger={refreshTrigger} />
    </ProtectedAction>
  </div>
);

export default WalletPage;

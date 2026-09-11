import ProtectedAction from "../components/ProtectedAction";
import AuthRequired from "../components/AuthRequired";
import StakingManager from "../components/StakingManager";

const StakingPage = ({ refreshTrigger, onUpdate }) => (
  <ProtectedAction fallback={<AuthRequired action="manage staking" />}>
    <StakingManager refreshTrigger={refreshTrigger} onUpdate={onUpdate} />
  </ProtectedAction>
);

export default StakingPage;

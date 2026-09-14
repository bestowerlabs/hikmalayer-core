import ProtectedAction from "../components/ProtectedAction";
import AuthRequired from "../components/AuthRequired";
import MiningActions from "../components/MiningActions";

const MiningPage = ({ refreshTrigger, onUpdate }) => (
  <ProtectedAction fallback={<AuthRequired action="access mining controls" />}>
    <MiningActions onUpdate={onUpdate} refreshTrigger={refreshTrigger} />
  </ProtectedAction>
);

export default MiningPage;

import ProtectedAction from "../components/ProtectedAction";
import AuthRequired from "../components/AuthRequired";
import CertificateManager from "../components/CertificateManager";

const CertificatesPage = ({ refreshTrigger, onUpdate }) => (
  <ProtectedAction fallback={<AuthRequired action="manage certificates" />}>
    <CertificateManager onUpdate={onUpdate} refreshTrigger={refreshTrigger} />
  </ProtectedAction>
);

export default CertificatesPage;


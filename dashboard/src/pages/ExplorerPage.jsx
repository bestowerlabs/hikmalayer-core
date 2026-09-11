import BlockchainViewer from "../components/BlockchainViewer";
import AssetExplorer from "../components/AssetExplorer";

const ExplorerPage = ({ refreshTrigger, onUpdate }) => (
  <div className="space-y-8">
    <AssetExplorer refreshTrigger={refreshTrigger} onUpdate={onUpdate} />
    <BlockchainViewer refreshTrigger={refreshTrigger} />
  </div>
);

export default ExplorerPage;

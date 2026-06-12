import { useState } from "react";
import TitleBar from "../organisms/TitleBar";
import ConnectScreen from "../templates/ConnectScreen";

export default function App() {
  const [isConnected, setIsconnected] = useState<boolean>(false);

  return (
    <div className="app-window">
      <TitleBar connected={isConnected} crumbs={[]} />

      <div className="app-body">
        {!isConnected && <ConnectScreen onConnected={() => {
          setIsconnected(true);
        }} />}
      </div>
    </div>
  );
}
import ConnectScreen from "./components/templates/ConnectScreen";

export default function App() {
  return (
    <div className="app-window">
      <div className="app-body">
        <ConnectScreen onConnected={() => alert('connecté !')} />
      </div>
    </div>
  );
}
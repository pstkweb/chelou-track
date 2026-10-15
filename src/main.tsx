import React from 'react';
import ReactDOM from 'react-dom/client';
import App from '@/components/pages/App';
import { initMediaToken } from '@/lib/stream';

// Media URLs are built synchronously at render time and need the token (Linux only).
initMediaToken().finally(() => {
  ReactDOM.createRoot(document.getElementById('app') as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
});

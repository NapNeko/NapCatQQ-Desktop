import React from 'react';
import ReactDOM from 'react-dom/client';
import { AppNext } from './app/AppNext';
import { AppProvidersNext } from './app/AppProvidersNext';

const root = ReactDOM.createRoot(document.getElementById('root') as HTMLElement);

root.render(
  <React.StrictMode>
    <AppProvidersNext>
      <AppNext />
    </AppProvidersNext>
  </React.StrictMode>,
);

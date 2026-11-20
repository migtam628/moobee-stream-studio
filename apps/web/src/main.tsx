import React from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import AlertOverlay from './overlay'
import './styles.css'

createRoot(document.getElementById('root')!).render(
  <React.StrictMode>{location.pathname.startsWith('/overlay/alerts') ? <AlertOverlay /> : <App />}</React.StrictMode>
)

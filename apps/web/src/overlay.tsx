import { useEffect, useState } from 'react'
import { Gift, Heart, Sparkles, Users } from 'lucide-react'
import type { Alert } from './types'

const icons = { follow: Heart, subscription: Sparkles, tip: Gift, raid: Users }

export default function AlertOverlay() {
  const [active, setActive] = useState<Alert | null>(null)
  useEffect(() => {
    const events = new EventSource('/api/events')
    let timer: ReturnType<typeof setTimeout>
    events.addEventListener('alert', (e) => {
      const alert = JSON.parse((e as MessageEvent).data) as Alert
      setActive(alert)
      clearTimeout(timer)
      timer = setTimeout(() => setActive(null), 5500)
    })
    return () => { clearTimeout(timer); events.close() }
  }, [])

  const Icon = active ? icons[active.type as keyof typeof icons] || Heart : Heart
  return <div className="overlay-root">
    {active && <div className="broadcast-alert" key={active.id}>
      <div className="broadcast-alert-icon"><Icon size={36} /></div>
      <p className="broadcast-alert-type">NEW {active.type.toUpperCase()}</p>
      <h2>{active.name}</h2>
      <p>{active.message}</p>
    </div>}
  </div>
}

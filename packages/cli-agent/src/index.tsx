import { createCliRenderer } from '@opentui/core'
import { render } from '@opentui/solid'
import App from './App'

async function main() {
  const renderer = await createCliRenderer({
    exitOnCtrlC: true,
    targetFps: 30,
    screenMode: 'alternate-screen',
    backgroundColor: '#0E1116',
    useMouse: true,
  })

  render(() => <App />, renderer)
}

main().catch((err) => {
  console.error('Failed to start Sentinel AI Agent:', err)
  process.exit(1)
})

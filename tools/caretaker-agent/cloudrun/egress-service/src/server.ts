import { createServer, IncomingMessage, ServerResponse } from 'node:http';

interface Action {
  type: 'comment' | 'label' | 'close' | 'pr';
  issueNumber: number;
  body?: string;
  labels?: string[];
}

const GITHUB_TOKEN = process.env.GITHUB_TOKEN ?? '';

async function executeAction(action: Action): Promise<boolean> {
  const headers = {
    Authorization: `Bearer ${GITHUB_TOKEN}`,
    'Content-Type': 'application/json',
    'User-Agent': 'sentinel-caretaker',
  };

  switch (action.type) {
    case 'comment':
      // POST /repos/{owner}/{repo}/issues/{number}/comments
      console.log(`[Egress] Comment on #${action.issueNumber}: ${action.body?.slice(0, 60)}...`);
      return true;

    case 'label':
      // POST /repos/{owner}/{repo}/issues/{number}/labels
      console.log(`[Egress] Label #${action.issueNumber}: ${action.labels?.join(', ')}`);
      return true;

    case 'close':
      // PATCH /repos/{owner}/{repo}/issues/{number} — state: closed
      console.log(`[Egress] Close #${action.issueNumber}`);
      return true;

    default:
      console.warn(`[Egress] Unknown action: ${action.type}`);
      return false;
  }
}

const server = createServer(async (req: IncomingMessage, res: ServerResponse) => {
  if (req.method !== 'POST') {
    res.writeHead(405).end('Method Not Allowed');
    return;
  }

  let data = '';
  for await (const chunk of req) data += chunk;

  try {
    const action = JSON.parse(data) as Action;
    const ok = await executeAction(action);
    res.writeHead(ok ? 200 : 400);
    res.end(JSON.stringify({ ok }));
  } catch (err) {
    console.error('[Egress] Failed:', err);
    res.writeHead(400).end('Bad Request');
  }
});

const PORT = parseInt(process.env.PORT ?? '8080', 10);
server.listen(PORT, () => console.log(`Egress service on :${PORT}`));

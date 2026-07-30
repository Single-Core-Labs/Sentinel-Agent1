import { createServer, IncomingMessage, ServerResponse } from 'node:http';

interface GitHubEvent {
  action: string;
  issue?: { number: number; title: string; body: string; user: { login: string } };
  pull_request?: { number: number; title: string; body: string };
  repository?: { full_name: string };
}

function normalizeEvent(body: GitHubEvent) {
  return {
    type: body.issue ? 'issue' : body.pull_request ? 'pull_request' : 'unknown',
    action: body.action,
    number: body.issue?.number ?? body.pull_request?.number,
    title: body.issue?.title ?? body.pull_request?.title,
    body: body.issue?.body ?? body.pull_request?.body,
    repo: body.repository?.full_name,
  };
}

const server = createServer(async (req: IncomingMessage, res: ServerResponse) => {
  if (req.method !== 'POST') {
    res.writeHead(405).end('Method Not Allowed');
    return;
  }

  let data = '';
  for await (const chunk of req) data += chunk;

  try {
    const event = JSON.parse(data) as GitHubEvent;
    const normalized = normalizeEvent(event);
    console.log(`[Ingestion] ${normalized.type}.${normalized.action} #${normalized.number}`);

    // Enqueue to Pub/Sub for triage worker
    // await pubsub.topic('triage').publish(Buffer.from(JSON.stringify(normalized)));

    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({ ok: true, id: normalized.number }));
  } catch (err) {
    console.error('[Ingestion] Failed:', err);
    res.writeHead(400).end('Bad Request');
  }
});

const PORT = parseInt(process.env.PORT ?? '8080', 10);
server.listen(PORT, () => console.log(`Ingestion service on :${PORT}`));

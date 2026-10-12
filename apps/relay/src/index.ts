/**
 * The signaling relay.
 *
 * A sharer's agent cannot be reached from a helper page that was loaded over HTTPS, and need not be:
 * both connect out to this relay instead, the agent to a room it names and the helper to the room it
 * was told. The relay passes their signaling messages along (pairing code, access requests, SDP) and
 * understands none of them. The screen, the input and the terminal never come here: they travel
 * directly between the two computers.
 *
 * One room is one Durable Object, which sleeps between messages while its sockets stay open.
 *
 * To the agent, each helper is a number:
 *   relay → agent   { from, joined: true, address, device? }   a helper page connected
 *                   { from, message }                          what it sent
 *                   { from, left: true }                       it went away
 *   agent → relay   { to, message }                            for that helper page
 *                   { to, close: true }                        turn it away
 * To a helper page, the relay is the agent: it sends and receives the plain messages.
 */

interface Env {
  ROOMS: DurableObjectNamespace;
  /** The sites whose pages may connect as a helper, separated by commas. */
  HELPER_ORIGINS: string;
}

/** As many pages as an agent lets wait with a pairing code. */
const MAX_HELPERS = 8;
/** Room for an SDP offer; nothing else that passes is near this size. */
const MAX_MESSAGE_BYTES = 64 * 1024;
/** A room's name is the random part of a help address: a few words joined by hyphens. */
const ROOM = /^[a-z]{2,10}(-[a-z]{2,10}){1,3}$/;

const refuse = (status: number, text: string) => new Response(text, { status });

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const [, role, room] = new URL(request.url).pathname.split("/");
    if ((role !== "agent" && role !== "helper") || !room || !ROOM.test(room)) return refuse(404, "Not found");
    if (request.headers.get("Upgrade")?.toLowerCase() !== "websocket") return refuse(426, "This is a WebSocket address");

    // A browser always says which site a page is from. An agent is not a browser and says nothing.
    const origin = request.headers.get("Origin");
    const allowed = env.HELPER_ORIGINS.split(",").map((site) => site.trim());
    if (role === "helper" && (!origin || !allowed.includes(origin))) return refuse(403, "Forbidden");
    if (role === "agent" && origin) return refuse(403, "Forbidden");

    return env.ROOMS.get(env.ROOMS.idFromName(room)).fetch(request);
  },
};

/** What the relay remembers about a socket, kept with the socket while the room sleeps. */
type Seat = { role: "agent" } | { role: "helper"; id: number };

export class Room {
  constructor(
    private readonly state: DurableObjectState,
    _env: Env,
  ) {
    // Answered without waking the room: an agent that waits for hours costs nothing.
    this.state.setWebSocketAutoResponse(new WebSocketRequestResponsePair("ping", "pong"));
  }

  private agent(): WebSocket | undefined {
    return this.state.getWebSockets("agent")[0];
  }

  private toAgent(message: unknown) {
    try {
      this.agent()?.send(JSON.stringify(message));
    } catch {
      // The agent has just gone; its close will follow.
    }
  }

  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);
    const role = url.pathname.split("/")[1];
    const { 0: client, 1: server } = new WebSocketPair();

    if (role === "agent") {
      // The room is whoever asked for help under this name first.
      if (this.agent()) return refuse(409, "This name is in use");
      this.state.acceptWebSocket(server, ["agent"]);
      server.serializeAttachment({ role: "agent" } satisfies Seat);
      return new Response(null, { status: 101, webSocket: client });
    }

    this.state.acceptWebSocket(server, ["helper"]);
    const helpers = this.state.getWebSockets("helper");
    if (!this.agent() || helpers.length > MAX_HELPERS) {
      // Said as an agent would say it, which the helper page already understands.
      server.send(JSON.stringify({ type: "error", reason: this.agent() ? "busy" : "not-asking" }));
      server.close(1000);
      return new Response(null, { status: 101, webSocket: client });
    }

    const taken = new Set(helpers.map((socket) => (socket.deserializeAttachment() as Seat | null)?.role === "helper" && (socket.deserializeAttachment() as { id: number }).id));
    let id = 0;
    do id = crypto.getRandomValues(new Uint32Array(1))[0]!;
    while (id === 0 || taken.has(id));
    server.serializeAttachment({ role: "helper", id } satisfies Seat);

    const device = url.searchParams.get("device");
    this.toAgent({
      from: id,
      joined: true,
      address: request.headers.get("CF-Connecting-IP") ?? "",
      ...(device && /^[0-9a-f]{1,64}$/.test(device) ? { device } : {}),
    });
    return new Response(null, { status: 101, webSocket: client });
  }

  webSocketMessage(socket: WebSocket, data: string | ArrayBuffer) {
    if (typeof data !== "string" || data.length > MAX_MESSAGE_BYTES) return;
    const seat = socket.deserializeAttachment() as Seat | null;

    if (seat?.role === "helper") {
      let message: unknown;
      try {
        message = JSON.parse(data);
      } catch {
        return;
      }
      return this.toAgent({ from: seat.id, message });
    }

    if (seat?.role === "agent") {
      let envelope: { to?: number; message?: unknown; close?: boolean };
      try {
        envelope = JSON.parse(data);
      } catch {
        return;
      }
      for (const helper of this.state.getWebSockets("helper")) {
        const there = helper.deserializeAttachment() as Seat | null;
        if (there?.role !== "helper" || there.id !== envelope.to) continue;
        try {
          if (envelope.message !== undefined) helper.send(JSON.stringify(envelope.message));
          if (envelope.close) helper.close(1000);
        } catch {
          // It has just gone; its close will follow.
        }
      }
    }
  }

  webSocketClose(socket: WebSocket) {
    const seat = socket.deserializeAttachment() as Seat | null;
    if (seat?.role === "helper" && seat.id) this.toAgent({ from: seat.id, left: true });
    // Without the agent there is nobody to talk to: the help request is over.
    if (seat?.role === "agent") {
      for (const helper of this.state.getWebSockets("helper")) {
        try {
          helper.close(1000);
        } catch {
          // Already closed.
        }
      }
    }
    try {
      socket.close(1000);
    } catch {
      // Already closed.
    }
  }

  webSocketError(socket: WebSocket) {
    this.webSocketClose(socket);
  }
}

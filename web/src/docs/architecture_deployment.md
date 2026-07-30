[Topic: arch-async]
# How a Solve Runs

frees is a client–server system with an **asynchronous compute model**. Understanding the five hops explains most of what you see in the UI — why the Solve button waits on a green Check, why long solves show a progress state instead of freezing the page, and why the server can scale to many concurrent users.

## The path of one solve

1. **Editor → API.** Pressing F2 sends your document text to the API node (`POST /api/solve`).
2. **Validate & enqueue.** The API node syntax-checks the text. If it parses, it pushes a compute task onto a **RabbitMQ** queue and immediately answers `202 Accepted` with a `jobId` — it never solves anything itself.
3. **Compute.** A **compute worker** picks the task off the queue and runs the full pipeline: parse (ANTLR) → expand matrices, CALLs, and component networks → unit check → Tarjan blocking → Newton solve (→ DYNAMIC integration, if present).
4. **Store.** The worker writes the result payload to **Redis** under the `jobId`.
5. **Poll & render.** The frontend polls `GET /api/jobs/{jobId}` (or subscribes to the job's event stream) until the state is `COMPLETED` or `FAILED`, then renders the Solution, Tables, and Plots panels from the payload.

## Why asynchronous?

- **No request timeouts.** A stiff transient or a deep parametric sweep can run for minutes; a synchronous HTTP request would time out at a proxy long before. A queued job runs as long as it needs.
- **Horizontal scale.** Compute workers are stateless queue consumers — add replicas and throughput scales; many users share one deployment without blocking each other.
- **Resilience.** If a worker dies mid-solve, the broker redelivers the task. frees treats a *redelivery* as evidence the job killed a worker and marks it `FAILED` instead of retrying it — a poison-message guard, so one pathological model can never crash-loop the compute tier.

## Check before Solve

`POST /api/check` runs everything *except* the solve: syntax, block expansion, unit verification, and structural solvability (degrees of freedom and a complete equation↔variable matching). It is fast and synchronous, which is why the editor gates the Solve button on a passing Check (F4) and re-requires it after any edit — structural errors are caught in milliseconds instead of a queued round-trip.

[Related: arch-api, deploy-docker, api]

[Topic: arch-api]
# The REST API

Everything the frontend does goes through the same public REST API — so anything the app can do, a script can do. Base path: `/api` (on a local Docker start, `http://localhost:8080/api`; through the frontend proxy, `http://localhost:5173/api`).

## Core endpoints

| Method & path | Purpose |
| --- | --- |
| `POST /api/check` | Validate syntax + structural solvability (synchronous) |
| `POST /api/solve` | Enqueue a solve → `202` + `jobId` |
| `POST /api/solve/table` | Enqueue a parametric-table solve |
| `GET  /api/jobs/{jobId}` | Poll job state and fetch the result payload |
| `GET  /api/jobs/{jobId}/stream` | Server-sent event stream of the job's progress |
| `POST /api/repl/evaluate` | Evaluate one REPL line against the cached session |
| `POST /api/optimize`, `/api/optimize/multi` | Single-objective / NSGA-II Pareto optimization |
| `POST /api/propplot`, `/api/psychart` | Property-chart / psychrometric-chart data |
| `POST /api/curve-fit` | Fit a model to tabulated data |
| `GET  /api/fluids` | The live supported-fluid list |
| `GET  /api/health` | Topology health (see *Health & Scaling*) |

## A solve from the command line

The request body's `text` field carries the document exactly as you would type it in the editor:

```
curl -s -X POST http://localhost:8080/api/solve \
  -H 'Content-Type: application/json' \
  -d '{"text": "P = 500 [kPa]\nVol = 0.05 [m^3]\nT = 25 [C]\nR = 0.287 [kJ/kg-K]\nP * Vol = m * R * T"}'
{ "jobId": "…" }        <- 202 Accepted

curl -s http://localhost:8080/api/jobs/<jobId>
{ "state": "COMPLETED", "solution": { … } }
```

Poll until `state` is `COMPLETED` or `FAILED`; the completed payload contains the same solution, table, and plot data the UI renders. This is the whole integration surface — batch studies, CI checks on engineering calcs, or a notebook driving frees remotely are all this pattern in a loop.

[Related: arch-async, deploy-health, repl]

[Topic: deploy-docker]
# Run Locally with Docker

The whole stack is containerized and managed by one script at the repository root — you never start or stop server processes by hand:

```
./frees.sh start      # build images if needed, start everything
./frees.sh status     # container status
./frees.sh logs       # follow logs
./frees.sh stop       # stop and remove containers
./frees.sh restart    # stop + start
./frees.sh build      # force a clean image rebuild
```

After `start`: the app is at **http://localhost:5173** and the API at **http://localhost:8080/api**.

## What comes up

`docker-compose.yml` wires the full topology:

| Service | Role |
| --- | --- |
| `frontend` | nginx serving the built React bundle, reverse-proxying `/api` to the API node |
| `api-node` | Spring Boot API tier — validates, enqueues, serves job status |
| `compute-node` | Spring Boot compute tier — consumes the queue and solves |
| `rabbitmq` | Task queue between the tiers |
| `redis` | Job store and solved-session cache |
| `otel-collector` + `jaeger` | Distributed tracing (optional, for development) |

A healthcheck makes the frontend wait until the backend is actually up. The backend image builds with Gradle in a multi-stage Dockerfile; the frontend builds the Vite bundle and serves it from nginx.

## Host-side development

Tests and dev servers run on the host, outside Docker:

```
cd backend  && ./gradlew test     # backend test suite
cd frontend && npm start          # Vite dev server (proxies /api to :8080)
cd frontend && npm run build      # type-check + production build
```

[Related: deploy-railway, deploy-health, arch-async]

[Topic: deploy-railway]
# Deploy to Railway

The same two images run unchanged on [Railway](https://railway.app) (or any container platform). A working deployment is five services — the two frees images plus managed Redis and RabbitMQ:

1. **backend (api)** — the backend image with the `api` Spring profile.
2. **backend (compute)** — the same image with the `compute` profile; scale replicas here for solve throughput.
3. **frontend** — the nginx image, with a public domain; it proxies `/api` over the private network to the API service.
4. **Redis** and **RabbitMQ** — Railway's managed templates; point the backend services at them with environment variables.

Only the frontend needs a public domain — the backend tiers, Redis, and RabbitMQ stay on the private network.

## Two production lessons (already baked in — keep them)

- **The frontend nginx re-resolves the backend address on every request.** On Railway's private network the backend's IP changes on every redeploy; a plain proxy configuration caches the address once at startup, so every backend redeploy used to hang `/api` behind 504s until the *frontend* restarted. The shipped `nginx.conf.template` uses a resolver with a variable upstream — if you touch the frontend proxy config, preserve that pattern.
- **The backend base image is pinned** (`eclipse-temurin:21-jre-noble`), because the floating `:21-jre` tag drifted to a distro whose SUNDIALS build is MPI-linked and aborts the JVM on the first transient solve. A build-time guard in the Dockerfile fails the image if that ever regresses. Don't "upgrade" the pin casually.

## Knowing what's deployed

The About dialog shows the exact git commit the running frontend was built from, linked to GitHub. On Railway this comes from the platform's `RAILWAY_GIT_COMMIT_SHA` at container start (locally, from a build argument) — so "is production actually running my fix?" is always one click to verify.

[Related: deploy-docker, deploy-health, arch-async]

[Topic: deploy-health]
# Health & Scaling

`GET /api/health` reports the **whole topology** in one call — each dependency with its own status plus replica counts:

- **api** — the node answering the request.
- **redis** / **rabbitmq** — connectivity to the job store and the broker.
- **compute** — how many workers are live, measured as actual consumers on the task queue (not a static config value). Zero consumers means solves will queue forever: that is the first thing to check when jobs sit in `PENDING`.
- **frontend** — reachability of the static tier.

The endpoint returns **200** when the system is `UP` or `DEGRADED` (something non-critical is down) and **503** when a critical dependency is `DOWN` — point your platform healthchecks and uptime monitors at it directly.

## Scaling the compute tier

Compute workers are stateless queue consumers: scale solve throughput by adding `compute` replicas, with no coordination or sticky state. Each solve occupies one worker for its duration, so size the tier to your expected concurrent-solve load.

## The poison-message guard

If a worker dies mid-job, RabbitMQ redelivers the task. A redelivered task is *presumed lethal* — the consumer marks it `FAILED` rather than solving it again, so one pathological model cannot take the whole tier down in a crash loop. The behavior is configurable (`frees.compute.drop-redelivered`) but on by default; leave it on in production.

[Related: arch-async, deploy-docker, deploy-railway]

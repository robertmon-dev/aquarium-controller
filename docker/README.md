# docker

InfluxDB for telemetry history, and Caddy serving the web app while proxying
read-only queries to Influx.

## Make targets

From the repo root, these wrap everything below:

| target | what |
| --- | --- |
| `make stack-env` | write `docker/.env` from the example, with generated password and admin token |
| `make stack-up` | build the web client, then start both services and wait for healthy |
| `make stack-down` | stop both services, keeping the data volumes |
| `make stack-ps` | what the stack is doing |
| `make stack-logs` | follow the logs |
| `make stack-tokens` | list the tokens and their permissions, values masked |
| `make stack-reset CONFIRM=yes` | destroy the containers **and** the Influx data |

`make stack-up` always rebuilds `apps/web/dist` first, because Caddy serves that
directory — otherwise it would quietly serve a stale build.

Creating the two scoped tokens is still manual, in step 5 below: it needs the
bucket id, which only exists after the first start.

## First run

1. `make stack-env` (or `cp .env.example .env` and fill it in by hand)
2. If you filled it by hand, set `INFLUX_PASSWORD` and `INFLUX_ADMIN_TOKEN` to
   fresh secrets: `openssl rand -hex 32`
3. `make stack-up` — the empty data volume triggers Influx's setup mode,
   which consumes every `DOCKER_INFLUXDB_INIT_*` variable **once**. Changing them
   later has no effect unless the `influx-data` volume is destroyed.
4. `docker compose exec influxdb influx bucket list` — note the bucket id.
5. Create the two scoped tokens:
   - `docker compose exec influxdb influx auth create --description "aquarium firmware write" --write-bucket <ID>`
   - `docker compose exec influxdb influx auth create --description "aquarium web proxy read" --read-bucket <ID>`

   Add `--json` to either if you want to script the token out of the output.
6. Put them in `.env` as `INFLUX_WRITE_TOKEN` and `INFLUX_READ_TOKEN`.
7. `docker compose up -d --force-recreate caddy` — Caddy reads the token at
   config-parse time, so it must be recreated, not reloaded. (`make stack-down`
   followed by `make stack-up` does the same thing.)
8. Put the firmware's values in the **repo-root** `.env`:
   `INFLUX_URL=http://<this-machine's-LAN-ip>:8086`, `INFLUX_ORG`,
   `INFLUX_BUCKET`, and `INFLUX_TOKEN=<the write token>`. Not `localhost` — the
   ESP32 resolves it on its own.

## Ports

| port | what |
| --- | --- |
| 8086 | Influx, published on the LAN because the ESP32 writes to it |
| 8080 | Caddy: the web app, plus `/influx/*` |

## What the proxy allows

Only `POST /influx/api/v2/query` and `GET /influx/ping`. Everything else under
`/influx/*` is 404. The injected token is read-only on one bucket, so the proxy
cannot be used to write, delete, or administer anything.

Anyone who can reach port 8080 on the LAN can read the history. There is no user
authentication, by design, for a home network.

## Three tokens, three jobs

| token | holder | permissions |
| --- | --- | --- |
| admin | local CLI only | everything |
| write | firmware, via repo-root `.env` | write to the bucket |
| read | Caddy, via `docker/.env` | read the bucket |

Verify the scoping with `docker compose exec influxdb influx auth list`: each
scoped token must show exactly **one** permission. A token that cannot read a
bucket gets `404 could not find bucket`, not `401` — Influx does not reveal the
existence of buckets you may not read.

## Querying

The browser talks to `/influx/api/v2/query` on the same origin, so it sends no
credentials. Ask for annotations explicitly, with a JSON body rather than
`application/vnd.flux`, or the response carries no column types:

```bash
curl -sS -X POST "http://localhost:8080/influx/api/v2/query?org=home" \
  -H "Content-Type: application/json" \
  -H "Accept: application/csv" \
  --data '{"query":"from(bucket:\"aquarium\") |> range(start:-1h)","dialect":{"annotations":["datatype","group","default"]}}'
```

Responses are CRLF-terminated.

## Resetting

`make stack-reset CONFIRM=yes` (or `docker compose down -v`) destroys the data
**and** the Influx setup, so the next start re-bootstraps and every token has to
be recreated. To keep the data, use `make stack-down`.

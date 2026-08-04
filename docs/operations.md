# Operations

## Modo de ejecución

El daemon actual está pensado para ejecutarse en foreground:

```bash
cargo run -- daemon
```

Hoy no incorpora su propio backgrounding. Si quieres dejarlo como proceso gestionado, necesitas envolverlo desde fuera con launchd, systemd, tmux, nohup o herramientas similares.

## Estado en disco

zproxy usa `~/.local/state/zproxy` para material de runtime:

- `zproxy.pid`: PID de la instancia activa.
- `zproxy.lock`: lockfile para impedir arranques simultáneos.
- `zproxy.sock`: socket Unix para control.

El código también reserva la ruta `zproxy.log`, aunque el logging actual sale por stdout o stderr del proceso.

## Socket de control

El socket Unix acepta un comando por línea. Los comandos implementados son:

- `status`
- `reload`
- `stop`

Normalmente no hace falta hablar con el socket a mano porque la CLI ya lo hace:

```bash
cargo run -- status
cargo run -- reload
cargo run -- stop
```

## Qué devuelve status

`status` resume:

- dirección de escucha
- upstream configurado
- fallback configurado
- gateway actual detectado, si aplica

Ejemplo:

```text
listen=127.0.0.1:8888 upstream=gateway:http:8080 fallback=direct gateway=192.168.1.1
```

## Recarga de configuración

`reload` vuelve a leer `~/.config/zproxy/config.toml` y sustituye la configuración en memoria. No reinicia el proceso; las conexiones nuevas pasan a usar la nueva configuración.

```bash
cargo run -- reload
```

La orden `cargo run -- config` guarda la config y después intenta exactamente esa misma recarga.

## Detección de gateway

Cuando `upstream.type = "gateway"`, el daemon mantiene un estado compartido con la IP del gateway por defecto:

- en macOS: `route -n get default`
- en Linux: `/proc/net/route` o `ip route show default`

La detección corre en una tarea separada y solo afecta a conexiones nuevas. No reencamina conexiones ya establecidas.

## Logging

El binario usa tracing. Por defecto, si no defines nada, arranca con un filtro equivalente a:

```text
info,zproxy=debug
```

Puedes ajustar el nivel con `RUST_LOG`:

```bash
RUST_LOG=debug cargo run -- daemon
RUST_LOG=trace cargo run -- daemon
RUST_LOG=warn cargo run -- daemon
```

## Fallos comunes

### El daemon no arranca porque ya existe otra instancia

El lockfile es exclusivo. Revisa si el proceso sigue vivo o si quedó basura en runtime después de un cierre abrupto.

Pasos típicos:

1. Comprueba el PID del proceso activo en `~/.local/state/zproxy/zproxy.pid`.
2. Si el proceso no existe, elimina lockfile, pidfile y socket obsoletos.
3. Vuelve a arrancar el daemon.

### status o reload no conectan con el socket

Suele significar una de estas cosas:

- el daemon no está corriendo
- el socket fue borrado
- hay desalineación entre HOME y las rutas esperadas

Usa primero:

```bash
cargo run -- paths
```

### El upstream gateway no resuelve nada

Comprueba manualmente que el comando del sistema devuelve gateway en esa máquina:

```bash
route -n get default
```

o en Linux:

```bash
ip route show default
```

## Límites actuales de operación

- No hay rotación de logs.
- No hay healthcheck persistente del upstream fuera del intento de conexión.
- No hay autenticación hacia proxies HTTP o SOCKS5.
- No hay endpoint de métricas.
- No hay comandos de administración extra aparte de status, reload y stop.
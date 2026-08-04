# Proxy intermediario en Rust para macOS — Diseño

## Objetivo

Aplicación macOS escrita en Rust que actúa como:

1. **Servidor proxy local** (HTTP/HTTPS vía `CONNECT`, opcionalmente SOCKS5).
2. **Cliente opcional de un proxy upstream**, con dos modos de resolución:
   - **Gateway dinámico**: `default_gateway_ip:<puerto>`, detectando automáticamente cambios de IP al cambiar de red.
   - **Estático**: host:puerto fijo.
3. **Fallback** configurable si el upstream principal no responde (otro proxy, o conexión directa al destino).

## Arquitectura

```
Cliente local → [App: escucha proxy en 127.0.0.1:PUERTO]
                     │
                     ├─ intenta upstream (gateway dinámico o estático)
                     │      └─ si falla → intenta fallback
                     │             └─ si falla → directo al destino (según config)
                     └─ si no hay upstream configurado → directo al destino
```

Componentes:

- **Listener/proxy handler**: acepta conexiones, gestiona `CONNECT` para HTTPS (tunneling, sin MITM inicialmente).
- **Gateway detector**: tarea independiente en background, actualiza un estado compartido (`Arc<RwLock<Option<IpAddr>>>`) con la IP del gateway actual.
- **Upstream resolver**: en cada conexión nueva, decide a qué destino conectar según config + estado del gateway detector.

## Crates propuestos

| Crate | Uso |
|---|---|
| `tokio` | runtime async, concurrencia de conexiones |
| `hyper` / `hyper-util` | servidor HTTP proxy, soporte de `CONNECT` |
| `tokio-socks` | cliente SOCKS5 si el upstream/fallback es SOCKS5 |
| `rustls` + `tokio-rustls` | TLS si en el futuro se añade MITM opcional |
| `system-configuration` | lectura reactiva de red en macOS (alternativa a polling con `route -n get default`) |
| `serde` + `toml` | configuración |
| `clap` | flags de CLI |

## Detección de gateway / cambio de IP

- **Opción simple (para empezar)**: ejecutar `route -n get default`, parsear el campo `gateway:`, comparar con el valor cacheado en un poller (`tokio::time::interval`, cada 5–10s).
- **Opción reactiva (optimización futura)**: `SCDynamicStore`/`SCNetworkReachability` vía crate `system-configuration`, para reaccionar a eventos de red sin polling.
- El detector corre como tarea separada del manejador de conexiones, para no bloquear conexiones activas al cambiar de red.

## Configuración (borrador)

```toml
[listen]
host = "127.0.0.1"
port = 8888

[upstream]
type = "gateway"          # "gateway" | "static" | "none"
port = 8080
poll_interval_secs = 5    # solo aplica si type = "gateway"

[fallback]
type = "static"           # "static" | "direct" | "none"
host = "1.2.3.4"
port = 8080
```

Lógica de conexión:

```
upstream_actual = resolver(config.upstream, gateway_state)
intentar conectar a upstream_actual
  si falla (timeout / connection refused):
    intentar fallback
      si falla:
        según config.fallback.type == "direct" → conectar directo al destino
        si no → error de conexión
```

## macOS: consideraciones adicionales

- Registrar la app como proxy del sistema: llamar a `networksetup` desde Rust, o dejar configuración manual en Ajustes de Red.
- Para correr en background como servicio: empaquetar como **LaunchAgent**.
- Para UI de configuración: opción **Tauri** (frontend TS/Vue + core Rust) o nativo SwiftUI + binario Rust vía FFI/XPC.

## Modo sin permisos de administrador (lanzador de shell)

Para equipos donde no hay permisos de admin (no se puede instalar como `launchd`/`systemd`), se soporta un modo alternativo: lanzador integrado en `.zshrc`/`.bashrc` que comprueba si el daemon está corriendo y, si no, lo arranca en background. En equipos donde sí hay permisos, se puede instalar como servicio real (ver tabla de la sección anterior); el binario del daemon es agnóstico a cómo se lanza.

### Lanzador en `.zshrc` / `.bashrc`

- Comprobación basada en **pidfile** (`~/.local/state/zproxy/zproxy.pid`) + `flock` sobre un lockfile, para evitar condiciones de carrera si se abren varias terminales a la vez.
- Al iniciar shell:
  1. ¿Existe el pidfile?
  2. ¿El PID está vivo (`kill -0`) y corresponde realmente a `zproxy` (evitar PIDs reciclados)?
  3. Si no, limpiar pidfile y relanzar.
- Arranque con `nohup zproxy daemon >> ~/.local/state/zproxy/log 2>&1 & disown` para que sobreviva al cierre de la terminal.
- El check debe ser barato (leer pidfile + `kill -0`), sin spawnear procesos pesados en cada shell nuevo.

### `zproxy config` — wizard interactivo + recarga en caliente

- Comando `zproxy config` lanza un wizard en terminal (crates candidatos: `dialoguer` o `inquire`) para configurar listen/upstream/fallback.
- Canal de control vía **socket Unix** (`~/.local/state/zproxy/zproxy.sock`) en vez de señales (`SIGHUP`), por ser más flexible y portable entre macOS/Linux:
  - Al guardar la config, el wizard escribe el TOML y envía un comando `reload` por el socket.
  - El mismo socket sirve para `zproxy status`, `zproxy stop`, etc.
  - Si el daemon no está corriendo, el wizard guarda el TOML sin notificar; el lanzador de shell lo recogerá en el próximo arranque.

### Distribución como servicio real (equipos con permisos)

- El binario del daemon corre en foreground; es `launchd`/`systemd` quien gestiona backgrounding, reinicio, logs, etc.
- Se pueden distribuir plantillas opcionales de unit files (`.service` para systemd, `.plist` para launchd) sin que el binario dependa de ellas.

## Multiplataforma: macOS + Linux

El core del proxy (`tokio`, `hyper`, `tokio-socks`, `rustls`, `serde`/`toml`, `clap`) es portable sin cambios. Solo las partes de sistema necesitan implementación por plataforma, idealmente detrás de un trait (p. ej. `GatewayDetector`) con `#[cfg(target_os = "...")]`:

| Función | macOS | Linux |
|---|---|---|
| Detección de gateway | `route -n get default` o `system-configuration` (SCDynamicStore) | leer `/proc/net/route`, o `ip route show default` |
| Registrar como proxy del sistema | `networksetup` | según entorno (GNOME: `gsettings`; KDE: `kwriteconfig`; o variables `http_proxy`/`https_proxy`) |
| Ejecutar como servicio | LaunchAgent/LaunchDaemon (`launchd`) | `systemd` unit |

**Compilación cruzada:**

- Compilar para Linux desde macOS: añadir target (`rustup target add x86_64-unknown-linux-gnu` / `aarch64-unknown-linux-gnu`) y usar `cross` (usa Docker por debajo) para evitar problemas de linking.
- Alternativa más simple: compilar nativo en cada plataforma vía CI, con matrix de `macos-latest` / `ubuntu-latest` en GitHub Actions.

## Próximos pasos / decisiones pendientes

- [ ] Definir estructura de crates del proyecto (workspace vs single crate).
- [ ] Elegir si el fallback puede ser en cascada (otro proxy con su propio fallback) o solo un nivel.
- [ ] Decidir si se necesitan reglas de bypass por dominio/IP (no pasar por proxy para ciertos destinos).
- [ ] Elegir estrategia de detección de gateway: polling simple vs `SCDynamicStore`.
- [ ] Decidir si habrá UI (Tauri) o solo CLI/daemon con LaunchAgent.
- [ ] Prototipo mínimo: servidor HTTP proxy con `hyper` + soporte `CONNECT`, sin upstream todavía.
- [ ] Diseñar trait `GatewayDetector` (u otro) para separar implementación por plataforma (macOS/Linux) desde el principio.
- [ ] Implementar lanzador de shell (pidfile + lock) y comando `zproxy daemon`.
- [ ] Implementar socket Unix de control (reload/status/stop) y wizard `zproxy config`.

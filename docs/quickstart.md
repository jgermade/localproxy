# Quickstart

## Requisitos

- Rust toolchain reciente con `cargo`.
- macOS o Linux.
- Acceso a red para descargar dependencias de crates.io en la primera compilación.

## Compilar

```bash
cargo build
```

Para un binario optimizado:

```bash
cargo build --release
```

## Ver rutas usadas por zproxy

```bash
cargo run -- paths
```

Salida esperada, con tus rutas reales:

```text
config: /Users/tu-usuario/.config/zproxy/config.toml
state: /Users/tu-usuario/.local/state/zproxy
socket: /Users/tu-usuario/.local/state/zproxy/zproxy.sock
pid: /Users/tu-usuario/.local/state/zproxy/zproxy.pid
```

## Crear la configuración inicial

```bash
cargo run -- config
```

El wizard pregunta por:

- host y puerto de escucha
- tipo de upstream: `none`, `gateway` o `static`
- protocolo del proxy upstream: `http` o `socks5`
- tipo de fallback: `none`, `direct` o `static`

Si el daemon ya está corriendo, el wizard intenta pedir una recarga automática por el socket de control.

## Arrancar el daemon

```bash
cargo run -- daemon
```

Qué hace este comando:

- crea las carpetas de config y estado si no existen
- toma un lock exclusivo para evitar varias instancias
- escribe pidfile
- arranca el detector de gateway si aplica
- abre el socket Unix de control
- abre el listener del proxy local

## Probar el proxy

Con salida directa:

```bash
curl -x http://127.0.0.1:8888 http://example.com
```

Con CONNECT para HTTPS:

```bash
curl -x http://127.0.0.1:8888 https://example.com
```

Si vas a probar otro puerto u host local, ajusta la URL del proxy a la configuración elegida.

## Consultar estado

En otra terminal:

```bash
cargo run -- status
```

Ejemplo de salida:

```text
listen=127.0.0.1:8888 upstream=none fallback=direct gateway=unknown
```

## Detener el daemon

```bash
cargo run -- stop
```

## Limitaciones prácticas del quickstart

- No hay integración automática con Ajustes de Red de macOS ni con el entorno de escritorio en Linux.
- No hay wrapper para dejarlo lanzado desde `.zshrc` o `.bashrc`.
- La primera compilación depende de poder descargar crates desde internet.
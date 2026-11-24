# Capsule-CHIP8

Un único ejecutable, `capsule-chip8`, que hace dos cosas:

- con `--pack`, genera un juego ejecutable a partir de un ROM CHIP-8;
- al ejecutar un juego generado, corre el emulador con el estado persistido
  dentro del propio exe.

El exe contiene el intérprete, el ROM y una **cápsula** con el estado completo
de la máquina (registros, RAM, stack, pantalla) y un disco que se puede leer,
escribir y hacer crecer físicamente en caliente.

## Por qué se re-arranca

Windows y Linux no permiten escribir el fichero que se está ejecutando. Por eso
el juego generado arranca como *launcher*: copia su imagen a un temporal, lanza
esa copia con `--inner <ruta original>` y la copia corre el emulador. El original
queda libre para reescribirse y ampliarse. `capsule-chip8` es esa misma imagen,
así que no hay plantillas ni binarios auxiliares: solo un exe.

El juego generado lleva dos secciones inyectadas:

- `.capsule`: cabecera `CAPSULE1`, ROM y dos snapshots de estado (A/B).
- `.disk`: el disco, 4 KiB al generar, crece en chunks de 4 KiB.

Los snapshots se alternan y todo se valida con CRC32.

## Compilar

```sh
cargo build --release
```

En Linux `minifb` necesita las cabeceras de X11/Wayland y `rodio` las de ALSA
(`libasound2-dev` en Debian/Ubuntu, `alsa-lib-devel` en Fedora).

## Generar y ejecutar

```sh
target/release/capsule-chip8 --pack --rom roms/test.ch8 --out roms/test-capsule.exe
roms/test-capsule.exe          # continúa donde se dejó
roms/test-capsule.exe --reset  # vuelve al estado de fábrica
```

Teclas: `1234 / qwer / asdf / zxcv`. `Escape` cierra y guarda. Cuando el sound
timer del Chip8 es mayor que cero suena un tono de 440 Hz; se apaga en cuanto
llega a cero. Si no hay dispositivo de audio, el emulador sigue corriendo sin
sonido.

## E/S de disco desde el ROM

| Dirección | Uso |
|---|---|
| `0x100`–`0x101` | Sector (u16 little-endian) |
| `0x102`–`0x111` | Buffer de 16 bytes |
| `0x112` | Comando |
| `0x113` | Estado: `0` = ok, `0xff` = error |

Comandos: `1` leer sector al buffer, `2` escribir buffer en sector, `3` devolver
número de sectores en `0x102`–`0x103`, `4` crecer el disco 4 KiB.

## Persistencia

- `DISK`: escritura **inmediata**, con `fsync`/`FlushFileBuffers` antes del ACK.
- `CPU/RAM/GFX`: checkpoint al final de cada frame (~60 Hz) y al salir.
- Todo se escribe a un temporal y se reemplaza de forma atómica. Si matas el
  proceso a mitad de escritura, el exe original sigue intacto. Los temporales
  huérfanos (incluidos los `~RF*.TMP` de Windows) se limpian al arrancar.

## Verificación

```sh
cargo run --release --example selftest -- roms/test-capsule.exe
```

Comprueba escritura/lectura de sector, tamaño, crecimiento físico del exe y que
el snapshot sigue siendo válido tras reabrir.

## Limitaciones conocidas

- El patrón auto-copia + exec + reescritura dispara falsos positivos de
  antivirus. Es inherente a "exe autocontenido" en Windows y Linux.
- El patcher PE respeta `VirtualSize` (debe cuadrar con la VA de la siguiente
  sección) y solo modifica `SizeOfRawData`.
- Un mutex nominal evita dos instancias del mismo juego a la vez.

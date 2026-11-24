# Capsule-CHIP8

Un único ejecutable, `capsule-chip8`, que hace dos cosas:

- con `--pack`, genera un juego ejecutable a partir de un ROM CHIP-8;
- al ejecutar un juego generado, corre el emulador con el estado persistido
  dentro del propio exe.

El exe contiene el intérprete, el ROM y una **cápsula** con el estado completo
de la máquina (registros, RAM, stack, timers y pantalla). El CHIP-8 no tiene
disco: solo se guarda eso para que el juego continúe donde lo dejó.

## Por qué se re-arranca

Windows y Linux no permiten escribir el fichero que se está ejecutando. Por eso
el juego generado arranca como *launcher*: copia su imagen a un temporal, lanza
esa copia con `--inner <ruta original>` y la copia corre el emulador. El original
queda libre para reescribirse. `capsule-chip8` es esa misma imagen, así que no
hay plantillas ni binarios auxiliares: solo un exe.

El juego generado lleva una sección inyectada, `.capsule`, con la cabecera
`CAPSULE1`, el ROM y dos snapshots de estado (A/B). Los snapshots se alternan y
todo se valida con CRC32.

## Compilar

```sh
cargo build --release
```

En Linux `minifb` necesita las cabeceras de X11/Wayland y `rodio` las de ALSA
(`libasound2-dev` en Debian/Ubuntu, `alsa-lib-devel` en Fedora).

## Generar y ejecutar

```sh
target/release/capsule-chip8 --pack --rom roms/Airplane.ch8 --out roms/airplane.exe
roms/airplane.exe          # continúa donde se dejó
roms/airplane.exe --reset  # vuelve al estado de fábrica
```

Teclas: `1234 / qwer / asdf / zxcv`. `Escape` cierra y guarda. Cuando el sound
timer del Chip8 es mayor que cero suena un tono de 440 Hz; se apaga en cuanto
llega a cero. Si no hay dispositivo de audio, el emulador sigue corriendo sin
sonido.

## Persistencia

- `CPU/RAM/GFX`: checkpoint al final de cada frame, al salir y al arrancar con
  `--reset`. El reloj de emulación va a ~600 instrucciones/s y los timers a
  60 Hz mediante un acumulador de tiempo, aunque la ventana no alcance 60 fps.
- Todo se escribe a un temporal y se reemplaza de forma atómica. Si matas el
  proceso a mitad de escritura, el exe original sigue intacto. Los temporales
  huérfanos (incluidos los `~RF*.TMP` de Windows) se limpian al arrancar.

## Verificación

```sh
cargo run --release --example selftest -- roms/test-capsule.exe
```

Comprueba que registros, RAM, stack, timers y pantalla sobreviven a dos
checkpoints consecutivos y que el exe no cambia de tamaño.

## Limitaciones conocidas

- El patrón auto-copia + exec + reescritura dispara falsos positivos de
  antivirus. Es inherente a "exe autocontenido" en Windows y Linux.
- El patcher PE respeta `VirtualSize` (debe cuadrar con la VA de la siguiente
  sección) y solo modifica `SizeOfRawData`.
- Un mutex nominal evita dos instancias del mismo juego a la vez.

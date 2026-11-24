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
`CAPSULE1` (versión 3), el mapa de controles, el ROM y dos snapshots de estado
(A/B). Los snapshots se alternan y todo se valida con CRC32.

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

En Windows el exe generado es una aplicación GUI: al abrirlo no aparece una
ventana de terminal. `--pack` y la ayuda siguen escribiendo en la consola.

Teclas por defecto: `1234 / qwer / asdf / zxcv`. `Escape` cierra y guarda.
Pulsa `F1` dentro del juego para abrir la pantalla de controles:

- `Izquierda`/`Derecha` eligen una de las 16 teclas CHIP-8.
- `Arriba`/`Abajo` recorren las teclas anfitrionas; también puedes pulsar
  directamente la tecla que quieres asignar (`0-9`, `A-Z`, espacio, `Tab`,
  `Shift`, `Ctrl`, `Alt`, `F2`-`F12`, signos y numérico). Las flechas, `F1`,
  `Enter` y `Escape` se usan para navegar y no se asignan pulsándolas; para
  asignar una flecha, usa `Arriba`/`Abajo` hasta que aparezca `UP`, `DN`, `LT`
  o `RT`.
- Si la tecla ya estaba asignada a otra ranura, se intercambian.
- `F1` o `Enter` guarda y vuelve al juego. `F1`, `Enter` y `Escape` no se
  pueden asignar.

El mapa se guarda en el propio exe, en posiciones fijas de la cabecera
`CAPSULE1`: 16 bytes en el offset 36 (uno por tecla CHIP-8) y su CRC32 en el
offset 52. `--reset` no borra los controles.

Cuando el sound timer del Chip8 es mayor que cero suena un tono de 440 Hz; se
apaga en cuanto llega a cero. Si no hay dispositivo de audio, el emulador sigue
corriendo sin sonido.

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

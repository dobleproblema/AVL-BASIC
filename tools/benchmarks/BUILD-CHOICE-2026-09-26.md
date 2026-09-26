# Configuración elegida tras las pruebas — 26 de septiembre de 2026

Se conservan los dos atributos Windows `link_section` y el perfil release
actual. No se introducen más cambios del intérprete ni opciones experimentales.

| Entorno | Rust/Cargo | Herramientas nativas | Ejecutable elegido |
| --- | --- | --- | --- |
| Windows x64 | 1.95.0 | MSVC 14.44, SDK 10.0.22000.0 | `rust195-new.exe` |
| WSL Ubuntu 24.04 x64 | 1.98.1 | Las registradas en la comparación Linux | `linux198` |

En Windows se mantiene Rust 1.95 porque 1.98.1 no aporta una mejora general y
empeora Pi con la separación de funciones actual. Se conserva el MSVC actualizado:
sus resultados son prácticamente iguales a los anteriores. En WSL, Rust 1.98.1
redujo el tiempo de Pi un 2,16 % y el de Jelly sin ventana un 4,57 %.

Evidencia: [disposición del código](CODE-PLACEMENT-2026-09-25.md),
[comparación de Rust](TOOLCHAIN-2026-09-25.md) y
[comparación de MSVC](MSVC-2026-09-26.md). La variante Windows elegida superó
765 pruebas; la de WSL, 775. Se reutilizan los mismos binarios medidos, sin
recompilarlos para este cierre. Esto no significa que se haya recuperado todo
el rendimiento histórico ni garantiza el de futuras compilaciones.

## Selección local por sistema

Se fija el compilador mediante un override de rustup para el directorio del
repositorio, independientemente en Windows y WSL. Los compiladores por defecto
del resto de proyectos permanecen como estaban. No se añade un
`rust-toolchain.toml` compartido que obligue a usar la misma versión en ambos
sistemas.

Desde la raíz del repositorio, para reproducir la selección Windows:

```powershell
rustup toolchain install 1.95.0 --profile minimal --no-self-update
rustup override set 1.95.0
rustc -Vv
```

Y en Linux/WSL:

```sh
rustup toolchain install 1.98.1 --profile minimal --no-self-update
rustup override set 1.98.1
rustc -Vv
```

Los scripts de empaquetado invocan `cargo` desde el repositorio y respetan
estos overrides. La selección se guarda localmente en rustup, no en Git;
otro checkout o equipo deberá configurarla expresamente. `rustup override unset`
desde el repositorio permite volver a la selección predeterminada de ese sistema.

En WSL se configuraron también las dos grafías locales `Documents/Rust/AVL-BASIC`
y `Documents/rust/AVL-BASIC`: acceden al mismo árbol en el volumen Windows,
pero rustup distingue esas rutas para sus overrides. Ambas seleccionan 1.98.1.
Esto afecta a `cargo`/`rustc`, no a ejecutar directamente un binario ya compilado.

El override fija Rust, no MSVC ni el SDK. Para reproducir una compilación Windows
de esta comparación, abrir `cmd`, ejecutar `VsDevCmd.bat` de Build Tools 2022
con las siguientes opciones y compilar desde la raíz del repositorio:

```bat
call "<Build Tools 2022>\Common7\Tools\VsDevCmd.bat" -no_logo -arch=x64 -host_arch=x64 -vcvars_ver=14.44.35207 -winsdk=10.0.22000.0
set "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER=%VCToolsInstallDir%bin\Hostx64\x64\link.exe"
set "CC_x86_64_pc_windows_msvc=%VCToolsInstallDir%bin\Hostx64\x64\cl.exe"
set "CXX_x86_64_pc_windows_msvc=%CC_x86_64_pc_windows_msvc%"
set "AR_x86_64_pc_windows_msvc=%VCToolsInstallDir%bin\Hostx64\x64\lib.exe"
cargo build --release --locked
```

Sustituir `<Build Tools 2022>` por la instalación local. En esta máquina se
comprobó además que la selección automática de Rust encuentra MSVC 14.44.
Hay varios SDK instalados; para comparar rendimiento debe fijarse el indicado,
en lugar de dar por supuesto que la selección automática coincide.

## Binarios de trabajo

Se copian los candidatos ya validados a `target/release/avl-basic.exe` y
`target/release/avl-basic`, con sus hashes verificados. El PDB Windows se sustituye
por el correspondiente al mismo EXE.

| Binario de trabajo | SHA-256 |
| --- | --- |
| Windows | `e012f173b42dde7328f0e212fbf147d407d51204e36ef2c6fae2f3dd6575d3e5` |
| WSL | `80910d551770f01b646f73917d2d5ab359ad024fc752860b28671e1b4696d5a3` |

Los binarios anteriores y el PDB se guardan en
`target/toolchain-comparison-20260925/before-final-selection/`. Los candidatos
y manifiestos de procedencia permanecen en el directorio superior. Todo ello
es salida local de compilación, fuera de Git.

Los informes de las pruebas describen el estado durante cada experimento;
esta nota registra la selección posterior. Los paquetes publicados y sus
versiones no se modifican en este cierre. Para el siguiente paquete se debe
comprobar el ejecutable exacto con Pi y Jelly, incluyendo Jelly con ventana,
como indica el [procedimiento de medición](README.md).

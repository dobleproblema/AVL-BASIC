# tests/conftest.py

import pytest
import subprocess
import os
import sys

ROOT_DIR = os.path.abspath(os.path.join(os.path.dirname(__file__), '..'))

@pytest.fixture
def run_basic_interpreter():
    """
    Fixture que ejecuta el intérprete BASIC con los comandos proporcionados y devuelve la salida.
    """
    def _run(commands, cwd=None):
        # Usar siempre '\n' y dejar que Python se encargue de traducirlo
        # al separador adecuado en cada plataforma (por ejemplo, '\r\n' en Windows).
        # Esto evita duplicar retornos de carro cuando se usa text=True en
        # subprocess.Popen.communicate(), lo que provocaba que algunas entradas
        # (p. ej. líneas en blanco) se interpretaran de forma errónea en Windows.
        input_commands = '\n'.join(commands) + '\n'

        # Ruta al intérprete BASIC
        interpreter_path = os.path.join(ROOT_DIR, 'basic.py')

        # Verificar si 'basic_graphics.py' existe
        if not os.path.isfile(interpreter_path):
            pytest.fail(f"No se encontró 'basic.py' en la ruta: {interpreter_path}")

        # Iniciar el intérprete BASIC
        try:
            popen_kwargs = {
                "stdin": subprocess.PIPE,
                "stdout": subprocess.PIPE,
                "stderr": subprocess.STDOUT,
                "text": True,
                "encoding": 'utf-8',
                "errors": 'strict',
            }
            if cwd is not None:
                popen_kwargs["cwd"] = str(cwd)
            process = subprocess.Popen(
                [sys.executable, "-X", "utf8", interpreter_path],  # -X utf8 por si acaso
                **popen_kwargs,
            )
        except FileNotFoundError:
            pytest.fail("No se encontró el intérprete de Python. Asegúrate de que Python está instalado y en el PATH.")
        except Exception as e:
            pytest.fail(f"Error al iniciar el intérprete: {e}")

        try:
            # Enviar todos los comandos y capturar la salida
            output, _ = process.communicate(input=input_commands, timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            output, _ = process.communicate()
            pytest.fail("El intérprete BASIC no respondió en el tiempo esperado (timeout).")
        except Exception as e:
            process.kill()
            pytest.fail(f"Error durante la comunicación con el intérprete: {e}")

        # Verificar si 'output' es None
        if output is None:
            pytest.fail("No se capturó ninguna salida del intérprete.")

        return output

    return _run

def pytest_sessionstart(session):
    """
    Hook que se ejecuta una sola vez, justo cuando pytest arranca.
    Garantiza que stdout y stderr usan UTF-8 para evitar � en la consola de VS Code.
    """
    for stream in (sys.stdout, sys.stderr):
        # stream.encoding puede ser None si está redirigido
        enc = (stream.encoding or "").lower()
        if enc != "utf-8":
            # Disponible desde Python 3.7
            stream.reconfigure(encoding="utf-8")

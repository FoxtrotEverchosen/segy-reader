import matplotlib
import numpy as np
from PyQt6.QtWidgets import QWidget, QVBoxLayout
from matplotlib.backends.backend_qtagg import FigureCanvasQTAgg
from matplotlib.figure import Figure
matplotlib.use("QtAgg")


class PlotCanvas(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)

        self.figure = Figure()
        self.canvas = FigureCanvasQTAgg(self.figure)
        self.ax = self.figure.add_subplot(111)

        layout = QVBoxLayout()
        layout.setContentsMargins(0, 0, 0, 0)
        layout.addWidget(self.canvas)
        self.setLayout(layout)

        self.ax.set_title("Trace View")
        self.ax.set_xlabel("Sample Index")
        self.ax.set_ylabel("Amplitude")

        self._line = None
        self.cbar = None

    def plot_trace(self, trace: np.ndarray):
        if self.cbar:
            self.cbar.remove()
            self.cbar = None

        self.ax.clear()
        x = np.arange(len(trace))
        self.ax.plot(x, trace, linewidth=0.8)

        self.ax.set_title("Trace View")
        self.ax.set_xlabel("Sample Index")
        self.ax.set_ylabel("Amplitude")

        self.ax.grid(True, alpha=0.3)
        self.canvas.draw_idle()

    def plot_section(self, section: np.ndarray):
        if self.cbar:
            self.cbar.remove()

        self.ax.clear()

        # Transpose so samples go down, traces go across
        data = section.T

        vmax = np.percentile(np.abs(data), 98)
        vmin = -vmax

        self._image = self.ax.imshow(
            data,
            aspect="auto",
            cmap="seismic",
            vmin=vmin,
            vmax=vmax,
            origin="upper"
        )

        self.ax.set_title("Seismic Section")
        self.ax.set_xlabel("Trace Number")
        self.ax.set_ylabel("Sample Index")

        self.cbar = self.figure.colorbar(self._image, ax=self.ax, shrink=0.8)
        self.canvas.draw_idle()

    def clear_plot(self):
        if self.cbar:
            self.cbar.remove()
            self.cbar = None

        self.ax.clear()
        self.canvas.draw_idle()

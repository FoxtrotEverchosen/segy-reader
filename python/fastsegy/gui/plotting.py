import matplotlib
import numpy as np
from PyQt6.QtWidgets import QWidget, QVBoxLayout
from matplotlib.backends.backend_qtagg import FigureCanvasQTAgg
from mpl_toolkits.axes_grid1 import make_axes_locatable
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
        self.cmap = "seismic"

    def plot_trace(self, sample_interval: float, trace: np.ndarray):
        if self.cbar:
            self.cbar.remove()
            self.cbar = None

        # sample_interval [us] -> [s]
        sample_interval = sample_interval / 1e6
        self.ax.clear()
        x = np.arange(0, len(trace) * sample_interval, sample_interval)
        self.ax.plot(x, trace, linewidth=0.8)

        self.ax.set_title("Trace View")
        self.ax.set_xlabel("Time [s]")
        self.ax.set_ylabel("Amplitude [-]")

        self.ax.grid(True, alpha=0.3)
        self.canvas.draw_idle()

    def plot_section(self, sample_interval: float, start_trace_index: int, data: np.ndarray):
        if self.cbar:
            self.cbar.remove()

        self.ax.clear()

        n_samples = data.shape[0]
        n_traces = data.shape[1]
        total_time = n_samples * sample_interval / 1e6
        x_start = start_trace_index
        x_end = start_trace_index + n_traces

        vmax = np.percentile(np.abs(data), 98)
        vmin = -vmax

        self._image = self.ax.imshow(
            data,
            aspect="auto",
            cmap=self.cmap,
            vmin=vmin,
            vmax=vmax,
            origin="upper",
            extent=(x_start, x_end, total_time, 0)
        )

        self.ax.set_title("Seismic Section")
        self.ax.set_xlabel("Trace Number")
        self.ax.set_ylabel("Time [s]")

        divider = make_axes_locatable(self.ax)
        cax = divider.append_axes("right", size="3%", pad=0.35)

        self.cbar = self.figure.colorbar(self._image, cax=cax)
        self.cbar.set_label("Amplitude [-]", loc="center")
        self.canvas.draw_idle()

    def clear_plot(self):
        if self.cbar:
            self.cbar.remove()
            self.cbar = None

        self.ax.clear()
        self.canvas.draw_idle()

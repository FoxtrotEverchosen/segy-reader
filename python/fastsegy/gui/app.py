import sys
import numpy as np

from PyQt6.QtWidgets import (
    QApplication,
    QMainWindow,
    QWidget,
    QVBoxLayout,
    QHBoxLayout,
    QLabel,
    QTableWidget,
    QTableWidgetItem,
    QMenuBar,
    QMenu,
    QDialog,
    QPushButton,
    QAbstractItemView,
    QMessageBox,
    QFileDialog, QLineEdit, QDialogButtonBox, QTextEdit
)
from PyQt6.QtCore import Qt
from pathlib import Path

from fastsegy import SegyFile, BinaryHeaderConfig, save_segy
from fastsegy.gui.plotting import PlotCanvas

from fastsegy.gui.function_dialogs import (
    ProfileFlipWindow,
    RunningAverageWindow,
    MedianXYFilterWindow
)

from fastsegy.processing import *

class SaveFileDialog(QDialog):
    def __init__(self, header_text: str, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Save Data As SEG-Y File")
        self.setModal(True)
        self.setMinimumWidth(500)

        layout = QVBoxLayout(self)

        path_label = QLabel("Output file path")
        layout.addWidget(path_label)

        path_layout = QHBoxLayout()
        self.path_edit = QLineEdit()
        self.path_edit.setPlaceholderText("Enter path or use Browse...")
        browse_btn = QPushButton("Browse")
        browse_btn.setFixedWidth(80)
        browse_btn.clicked.connect(self.browse)
        path_layout.addWidget(self.path_edit)
        path_layout.addWidget(browse_btn)
        layout.addLayout(path_layout)

        header_label = QLabel("Textual header (editable)")
        layout.addWidget(header_label)

        self.header_edit = QTextEdit()
        self.header_edit.setPlaceholderText("Textual header...")
        self.header_edit.setText(header_text)
        self.header_edit.setLineWrapMode(QTextEdit.LineWrapMode.NoWrap)
        # ~80 chars wide using monospace font so char width is predictable
        self.header_edit.setFontFamily("Monospace")
        font_metrics = self.header_edit.fontMetrics()
        self.header_edit.setFixedWidth(font_metrics.averageCharWidth() * 84)  # 80 chars + padding
        self.header_edit.setMinimumHeight(200)
        layout.addWidget(self.header_edit)

        buttons_layout = QHBoxLayout()
        ok_btn = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok, self)
        cancel_btn = QDialogButtonBox(QDialogButtonBox.StandardButton.Cancel, self)
        buttons_layout.addStretch()
        buttons_layout.addWidget(ok_btn)
        buttons_layout.addWidget(cancel_btn)
        layout.addLayout(buttons_layout)

        ok_btn.accepted.connect(self.accept)
        cancel_btn.rejected.connect(self.reject)

    def browse(self):
        path, _ = QFileDialog.getSaveFileName(
            self,
            "Save SEG-Y File",
            str(Path.home()),
            "SEG-Y files (*.sgy *.segy *.seg)"
        )
        if path:
            self.path_edit.setText(path)

    def get_path(self) -> str:
        return self.path_edit.text().strip()

    def get_header(self) -> str:
        return self.header_edit.toPlainText()


class FunctionWindow(QDialog):
    def __init__(self, function_name):
        super().__init__()
        self.setWindowTitle(function_name)
        self.setMinimumSize(300, 150)

        layout = QVBoxLayout()

        label = QLabel(f"Settings for function:\n\n{function_name}")
        label.setAlignment(Qt.AlignmentFlag.AlignCenter)

        close_button = QPushButton("Close")
        close_button.clicked.connect(self.close)

        layout.addWidget(label)
        layout.addWidget(close_button)

        self.setLayout(layout)


class TextHeaderWindow(QDialog):
    def __init__(self, header_text: str, parent=None):
        super().__init__(parent)

        self.setWindowTitle("SEG-Y Textual Header")
        self.setMinimumSize(600, 400)

        layout = QVBoxLayout()

        self.text_view = QTextEdit()
        self.text_view.setReadOnly(True)
        self.text_view.setLineWrapMode(QTextEdit.LineWrapMode.NoWrap)
        self.text_view.setText(header_text)

        layout.addWidget(self.text_view)

        button_layout = QHBoxLayout()
        close_button = QPushButton("Close")
        close_button.setFixedWidth(120)

        close_button.clicked.connect(self.close)

        button_layout.addStretch()
        button_layout.addWidget(close_button)
        button_layout.addStretch()

        layout.addLayout(button_layout)

        self.setLayout(layout)


class App(QMainWindow):
    def __init__(self):
        super().__init__()

        #TODO: some of those are no longer necessary
        self.functions_table = None
        self.data_table = None
        self.canvas = None
        self.segy_file = None
        self.metadata = None
        self.trace_data = None
        self.sample_interval = None
        self.trace_data_shape = None
        self.trace_data_range = None
        self.file_name = None
        self.file_path = None
        self.setWindowTitle("FastSegy App")
        self.setMinimumSize(1000, 700)
        self.create_menu()
        self.create_layout()

        self.function_map = {
            "Flip Profile": (ProfileFlipWindow, profile_flip),
            "Running Average": (RunningAverageWindow, running_average),
            "Median XY-Filter": (MedianXYFilterWindow, median_xy_filter),
        }

    def create_menu(self):
        menubar = QMenuBar(self)
        self.setMenuBar(menubar)

        file_menu = QMenu("File", self)
        menubar.addMenu(file_menu)

        file_menu.addAction("Open SEG-Y File", self.open_file_dialog)
        file_menu.addAction("Close SEG-Y File", self.drop_file)

        # edit_menu = QMenu("Edit", self)
        # menubar.addMenu(edit_menu)
        #
        # edit_menu.addAction("Preferences")
        # edit_menu.addAction("Clear Canvas")

        data_menu = QMenu("Data", self)
        menubar.addMenu(data_menu)

        data_menu.addAction("Get Trace", self.trace_dialog)
        data_menu.addAction("Get Trace Range", self.trace_range_dialog)
        data_menu.addAction("Get Textual Header", self.get_text_header)
        data_menu.addAction("Save Data Buffer As File", self.save_data)

    def open_file_dialog(self):
        home_dir = str(Path.home())
        path = QFileDialog.getOpenFileName(self, 'Open File', home_dir, filter="SEG-Y files (*.seg *.segy *.sgy)")[0]

        if path:
            file_name = Path(path).name
            self.file_name = file_name
            self.file_path = path
            self.segy_file = SegyFile(path)
            self.metadata = self.segy_file.get_metadata()
            self.populate_data_table()
            self.sample_interval = float(self.metadata.get("Sample Interval"))

    def drop_file(self):
        self.segy_file = None
        self.metadata = None
        self.trace_data = None
        self.file_name = None
        self.file_path = None

        self.data_table.setRowCount(0)
        self.data_table.setRowCount(40)

        placeholder_data = [
            ("-", "-"),
            ("-", "-"),
            ("-", "-"),
            ("-", "-"),
            ("-", "-"),
        ]

        for row, (key, value) in enumerate(placeholder_data):
            self.data_table.setItem(row, 0, QTableWidgetItem(key))
            self.data_table.setItem(row, 1, QTableWidgetItem(value))

        if self.canvas is not None:
            self.canvas.clear_plot()

    def trace_dialog(self):
        dialog = QDialog(self)
        dialog.setWindowTitle("Select a trace to be shown")
        dialog.setModal(True)

        layout = QVBoxLayout(dialog)
        num_label = QLabel("Trace number")
        num_edit = QLineEdit()
        layout.addWidget(num_label)
        layout.addWidget(num_edit)

        buttons_layout = QHBoxLayout()
        ok_btn = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok, dialog)
        cancel_btn = QDialogButtonBox(QDialogButtonBox.StandardButton.Cancel, dialog)

        buttons_layout.addStretch()
        buttons_layout.addWidget(ok_btn)
        buttons_layout.addWidget(cancel_btn)

        layout.addLayout(buttons_layout)
        cancel_btn.clicked.connect(lambda: dialog.close())

        ok_btn.accepted.connect(dialog.accept)
        if dialog.exec() == 1:
            if self.segy_file is None:
                self.show_warning("To request a trace data a file must be first loaded!")
                return

            try:
                value = int(num_edit.text())
                self.trace_data = self.segy_file.get_trace(value)
                self.trace_data_shape = np.shape(self.trace_data)
                self.trace_data_range = None
                self.canvas.plot_trace(self.sample_interval, self.trace_data)
            except Exception as e:
                self.show_error(str(e))

    def trace_range_dialog(self):
        dialog = QDialog(self)
        dialog.setWindowTitle("Select a trace range to be shown")
        dialog.setModal(True)

        layout = QVBoxLayout(dialog)
        start_label = QLabel("Starting trace (1-based)")
        start_edit = QLineEdit()

        end_label = QLabel("Ending trace (1-based)")
        end_edit = QLineEdit()

        layout.addWidget(start_label)
        layout.addWidget(start_edit)
        layout.addWidget(end_label)
        layout.addWidget(end_edit)

        buttons_layout = QHBoxLayout()
        ok_btn = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok, dialog)
        cancel_btn = QDialogButtonBox(QDialogButtonBox.StandardButton.Cancel, dialog)

        buttons_layout.addStretch()
        buttons_layout.addWidget(ok_btn)
        buttons_layout.addWidget(cancel_btn)

        layout.addLayout(buttons_layout)
        cancel_btn.clicked.connect(lambda: dialog.close())

        ok_btn.accepted.connect(dialog.accept)
        if dialog.exec() == 1:
            if self.segy_file is None:
                self.show_warning("To request a trace range data a file must be first loaded!")
                return

            try:
                start = int(start_edit.text())
                end = int(end_edit.text())

                # Transpose data for better visualisation
                self.trace_data = self.segy_file.get_trace_range(start, end).T
                self.trace_data_shape = np.shape(self.trace_data)
                self.trace_data_range = (start, end)
                self.canvas.plot_section(self.sample_interval, start, self.trace_data)
            except Exception as e:
                self.show_error(str(e))

    def populate_data_table(self):
        self.data_table.setItem(0, 0, QTableWidgetItem("File Name"))
        self.data_table.setItem(0, 1, QTableWidgetItem(str(self.file_name)))

        for i, kv_pair in enumerate(self.metadata.items()):
            if kv_pair[0] == 'Index':
                continue

            self.data_table.setItem(i+1, 0, QTableWidgetItem(str(kv_pair[0])))
            self.data_table.setItem(i+1, 1, QTableWidgetItem(str(kv_pair[1])))

    def create_layout(self):
        central_widget = QWidget()
        self.setCentralWidget(central_widget)

        main_layout = QHBoxLayout()
        central_widget.setLayout(main_layout)

        self.canvas = PlotCanvas()
        main_layout.addWidget(self.canvas, stretch=3)

        right_panel = QVBoxLayout()

        self.data_table = self.create_data_table()
        right_panel.addWidget(self.data_table, stretch=2)

        self.functions_table = self.create_functions_table()
        right_panel.addWidget(self.functions_table, stretch=1)

        main_layout.addLayout(right_panel, stretch=1)

    def create_data_table(self):
        table = QTableWidget()
        table.setColumnCount(2)
        table.setRowCount(40)

        table.setHorizontalHeaderLabels(["Property", "Value"])
        table.setEditTriggers(QAbstractItemView.EditTrigger.NoEditTriggers)
        table.setSelectionMode(QAbstractItemView.SelectionMode.NoSelection)

        placeholder_data = [
            ("-", "-"),
            ("-", "-"),
            ("-", "-"),
            ("-", "-"),
            ("-", "-"),
        ]

        for row, (key, value) in enumerate(placeholder_data):
            table.setItem(row, 0, QTableWidgetItem(key))
            table.setItem(row, 1, QTableWidgetItem(value))

        table.setSizePolicy(
            table.sizePolicy().horizontalPolicy(),
            table.sizePolicy().verticalPolicy(),
        )

        table.horizontalHeader().setStretchLastSection(True)
        table.verticalHeader().setVisible(False)
        table.setStyleSheet("QTableWidget { border: 2px solid black; }")

        return table

    def create_functions_table(self):
        table = QTableWidget()
        table.setColumnCount(1)
        table.setRowCount(3)

        table.setHorizontalHeaderLabels(["Functions"])

        functions = [
            "Flip Profile",
            "Running Average",
            "Median XY-Filter",
        ]

        for row, name in enumerate(functions):
            table.setItem(row, 0, QTableWidgetItem(name))

        table.setSizePolicy(
            table.sizePolicy().horizontalPolicy(),
            table.sizePolicy().verticalPolicy(),
        )

        table.horizontalHeader().setStretchLastSection(True)
        table.verticalHeader().setVisible(False)
        table.cellClicked.connect(self.open_function_window)
        table.setStyleSheet("QTableWidget { border: 2px solid black; }")

        return table

    def open_function_window(self, row):
        if self.trace_data is None:
            self.show_warning("Load or request data before applying functions.")
            return

        item = self.functions_table.item(row, 0)
        if not item:
            return

        name = item.text()
        dialog, transformation = self.function_map.get(name)
        dialog = dialog(self)

        if dialog.exec():
            params = dialog.get_params()

            try:
                self.trace_data = transformation(params, self.trace_data, self.sample_interval)
                self.canvas.plot_section(self.sample_interval, self.trace_data_range[0], self.trace_data)
            except Exception as e:
                self.show_error("Encountered error while transforming data, processed has not finished,"
                                f" data remained unchanged. Error message: \n {e}")

    def get_text_header(self):
        if self.segy_file is None:
            self.show_warning("No file loaded!")
            return

        header_text = self.segy_file.get_header()
        dlg = TextHeaderWindow(header_text, self)
        dlg.exec()

    def save_data(self):
        if self.trace_data is None or len(self.trace_data.shape) == 1:
            self.show_error("You can only save range of data that is currently displayed and in buffer!")
            return

        dialog = SaveFileDialog(self.segy_file.get_header(), self)
        if not dialog.exec():
            return

        file_path = dialog.get_path()
        if not file_path:
            self.show_warning("No file path provided.")
            return

        # Ensure extension
        if not any(file_path.endswith(ext) for ext in (".sgy", ".segy", ".seg")):
            file_path += ".sgy"

        textual_header = dialog.get_header().replace("\n", "")

        # Data will always be saved as f64 with text encoded as ASCII, following Revision 1.0
        DATA_FORMAT = 6
        BYTES_PER_SAMPLE = 8
        REVISION_STANDARD = 0x0100
        IS_ASCII = True
 
        BYTE_ORDER = {
            "Big Endian":   0x01_02_03_04,
            "Little Endian":0x04_03_02_01,
            "Swapped Word": 0x02_01_04_03,
        }

        conf = BinaryHeaderConfig(
            self.metadata["Sample Interval"],
            self.metadata["Samples Per Trace"],
            DATA_FORMAT,
            REVISION_STANDARD,
            0,
            BYTE_ORDER.get(self.metadata["Byte Order"]),
            BYTES_PER_SAMPLE
        )
        traces = self.trace_data.T  # undo the transpose done at load time → (n_traces, n_samples)
        n_traces = traces.shape[0]
        n_samples = traces.shape[1]

        traces = traces.astype(np.float64)
        if self.metadata["Byte Order"] == "Big Endian":
            traces = traces.astype(traces.dtype.newbyteorder('>'))

        try:
            save_segy(file_path, textual_header, conf, traces.tobytes(), IS_ASCII, n_traces)
            QMessageBox.information(self, "Saved", f"File saved to:\n{file_path}")
        except Exception as e:
            self.show_error(f"Failed to save file:\n{e}")


    def show_error(self, message):
        QMessageBox.critical(self, "FastSegy Error", message)

    def show_warning(self, message):
        QMessageBox.warning(self, "FastSegy Warning", message)


def main():
    app = QApplication(sys.argv)
    window = App()
    window.show()
    sys.exit(app.exec())


if __name__ == "__main__":
    main()


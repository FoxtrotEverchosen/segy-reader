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

from fastsegy import SegyFile
from fastsegy.gui.plotting import PlotCanvas

from fastsegy.gui.function_dialogs import (
    ProfileFlipWindow,
    RunningAverageWindow,
    MedianXYFilterWindow
)

from fastsegy.processing import *


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
        self.functions_table = None
        self.data_table = None
        self.canvas = None
        self.segy_file = None
        self.metadata = None
        self.trace_data = None
        self.sample_interval = None
        self.trace_data_shape = None
        self.trace_data_range = None
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

        file_menu.addAction("Open SEG-Y", self.open_file_dialog)
        file_menu.addAction("Close SEG-Y file", self.drop_file)

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

    def open_file_dialog(self):
        home_dir = str(Path.home())
        path = QFileDialog.getOpenFileName(self, 'Open file', home_dir, filter="SEG-Y files (*.seg *.segy)")[0]

        if path:
            self.segy_file = SegyFile(path)
            self.metadata = self.segy_file.get_metadata()
            self.populate_data_table(self.metadata)
            self.sample_interval = float(self.metadata.get("Sample Interval"))

    def drop_file(self):
        self.segy_file = None
        self.metadata = None
        self.trace_data = None

        placeholder_data = [
            ("Samples Per Trace", "—"),
            ("Bytes Per Sample", "—"),
            ("Data Format", "—"),
            ("Byte Order", "—"),
            ("Trace Count", "—"),
            ("Sample Interval", "—"),
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

                if end - start > 1500:
                    self.show_warning("Due to memory limitations, a user can request up to 1500 traces!")
                    return

                # Transpose data for better visualisation
                self.trace_data = self.segy_file.get_trace_range(start, end).T
                self.trace_data_shape = np.shape(self.trace_data)
                self.trace_data_range = (start, end)
                self.canvas.plot_section(self.sample_interval, start, self.trace_data)
            except Exception as e:
                self.show_error(str(e))

    def populate_data_table(self, metadata):
        rows = self.data_table.rowCount()

        for i in range(rows):
            key_type = self.data_table.item(i, 0)
            if key_type is None:
                break
            key_text = key_type.text()

            value = self.metadata.get(key_text, "Undefined")
            self.data_table.setItem(i, 1, QTableWidgetItem(str(value)))

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
        table.setRowCount(20)

        table.setHorizontalHeaderLabels(["Property", "Value"])
        table.setEditTriggers(QAbstractItemView.EditTrigger.NoEditTriggers)
        table.setSelectionMode(QAbstractItemView.SelectionMode.NoSelection)

        placeholder_data = [
            ("Samples Per Trace", "—"),
            ("Bytes Per Sample", "—"),
            ("Data Format", "—"),
            ("Byte Order", "—"),
            ("Trace Count", "—"),
            ("Sample Interval", "—"),
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

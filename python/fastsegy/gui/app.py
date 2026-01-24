import sys

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
    QFileDialog
)
from PyQt6.QtCore import Qt
from pathlib import Path

from fastsegy import SegyFile


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


class App(QMainWindow):
    def __init__(self):
        super().__init__()
        self.functions_table = None
        self.data_table = None
        self.canvas = None
        self.segy_file = None
        self.metadata = None
        self.setWindowTitle("FastSegy App")
        self.setMinimumSize(1000, 700)
        self.create_menu()
        self.create_layout()

    def create_menu(self):
        menubar = QMenuBar(self)
        self.setMenuBar(menubar)

        file_menu = QMenu("File", self)
        menubar.addMenu(file_menu)

        file_menu.addAction("Open SEG-Y", self.open_file_dialog)
        file_menu.addAction("Close SEG-Y file", self.drop_file)

        edit_menu = QMenu("Edit", self)
        menubar.addMenu(edit_menu)

        edit_menu.addAction("Preferences", lambda: print(self.segy_file.get_trace(3500)))
        edit_menu.addAction("Clear Canvas")

    def open_file_dialog(self):
        home_dir = str(Path.home())
        path = QFileDialog.getOpenFileName(self, 'Open file', home_dir, filter="SEG-Y files (*.seg *.segy)")[0]

        if path:
            self.segy_file = SegyFile(path)
            self.metadata = self.segy_file.get_metadata()
            self.populate_details(self.metadata)

    def drop_file(self):
        self.segy_file = None

    def populate_details(self, metadata):
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

        self.canvas = QLabel("Canvas for drawing SEG-Y data")
        self.canvas.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.canvas.setStyleSheet(
            """
            QLabel {
                border: 2px solid black;
                background-color: white;
                font-size: 16px;
            }
            """
        )

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
            "Gain Control",
            "Bandpass Filter",
            "Normalize Traces",
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
        item = self.functions_table.item(row, 0)
        if not item:
            return

        function_name = item.text()
        popup = FunctionWindow(function_name)
        popup.exec()


def main():
    app = QApplication(sys.argv)
    window = App()
    window.show()
    sys.exit(app.exec())


if __name__ == "__main__":
    main()

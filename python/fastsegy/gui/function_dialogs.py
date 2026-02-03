from PyQt6.QtCore import Qt
from PyQt6.QtWidgets import (
    QDialog, QVBoxLayout, QLabel, QLineEdit,
    QDialogButtonBox, QRadioButton
)


class ProfileFlipWindow(QDialog):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Profile Flip")
        self.setMinimumSize(300, 150)

        layout = QVBoxLayout(self)

        desc = QLabel(
            "Flips the seismic profile along the selected axis.\n\n"
            "X-Flip: Reverses the trace order along X axis\n"
            "Y-Flip: Reverses the time/sample direction along Y axis"
        )
        desc.setWordWrap(True)
        desc.setAlignment(Qt.AlignmentFlag.AlignTop)
        layout.addWidget(desc)

        self.x_radio = QRadioButton("X-Flip (Traces)")
        self.y_radio = QRadioButton("Y-Flip (Samples)")
        self.x_radio.setChecked(True)

        layout.addWidget(self.x_radio)
        layout.addWidget(self.y_radio)

        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok |
            QDialogButtonBox.StandardButton.Cancel
        )

        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)

        layout.addWidget(buttons)

    def get_params(self):
        return {
            "axis": "x" if self.x_radio.isChecked() else "y"
        }


class RunningAverageWindow(QDialog):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Running Average")
        self.setMinimumSize(300, 200)

        layout = QVBoxLayout(self)

        desc = QLabel(
            "Applies a horizontal running average across neighboring traces.\n\n"
            "This filter enhances laterally continuous reflectors and suppresses "
            "trace-dependent noise by averaging amplitudes across adjacent traces "
            "for each time sample.\n\n"
            "Parameters:\n"
            "• Average Range — Number of neighboring traces used for averaging\n"
            "• Start Time — First sample index where the filter is applied\n"
            "• End Time — Last sample index where the filter is applied- if left empty, the filter will be applied to"
            " the whole rest of trace data"
        )
        desc.setWordWrap(True)
        layout.addWidget(desc)

        self.range_edit = QLineEdit()
        self.start_edit = QLineEdit()
        self.end_edit = QLineEdit()

        layout.addWidget(QLabel("Average Range"))
        layout.addWidget(self.range_edit)

        layout.addWidget(QLabel("Start Sample"))
        layout.addWidget(self.start_edit)

        layout.addWidget(QLabel("End Sample"))
        layout.addWidget(self.end_edit)

        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok |
            QDialogButtonBox.StandardButton.Cancel
        )

        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)

        layout.addWidget(buttons)

    def get_params(self):
        return {
            "range": self.range_edit.text(),
            "start_time": self.start_edit.text(),
            "end_time": self.end_edit.text()
        }


class MedianXYFilterWindow(QDialog):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("Median XY-Filter")
        self.setMinimumSize(300, 220)

        layout = QVBoxLayout(self)
        self.x_edit = QLineEdit()
        self.y_edit = QLineEdit()
        self.start_edit = QLineEdit()
        self.end_edit = QLineEdit()

        desc = QLabel(
            "Applies a 2D median filter across traces (X) and samples (Y).\n\n"
            "This filter suppresses spike noise and trace-dependent artifacts by "
            "replacing each sample with the median value from a local XY window.\n\n"
            "Parameters:\n"
            "• Traces (X) — Number of neighboring traces used\n"
            "• Samples (Y) — Number of neighboring time samples used\n"
            "• Start Time — First sample index where the filter is applied\n"
            "• End Time — Last sample index where the filter is applied - if left empty, the filter will be applied to"
            " the whole rest of trace data"
        )
        desc.setWordWrap(True)
        desc.setAlignment(Qt.AlignmentFlag.AlignTop)
        layout.addWidget(desc)

        layout.addWidget(QLabel("Traces (X)"))
        layout.addWidget(self.x_edit)

        layout.addWidget(QLabel("Samples (Y)"))
        layout.addWidget(self.y_edit)

        layout.addWidget(QLabel("Start Time"))
        layout.addWidget(self.start_edit)

        layout.addWidget(QLabel("End Time"))
        layout.addWidget(self.end_edit)

        buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Ok |
            QDialogButtonBox.StandardButton.Cancel
        )

        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)

        layout.addWidget(buttons)

    def get_params(self):
        return {
            "x": self.x_edit.text(),
            "y": self.y_edit.text(),
            "start_time": self.start_edit.text(),
            "end_time": self.end_edit.text()
        }

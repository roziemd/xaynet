import os
from typing import Dict, List, Optional, Tuple

import numpy as np
from absl import app, flags, logging

from xain.types import PlotValues, XticksLabels, XticksLocations

from .aggregation import GroupResult, TaskResult
from .plot import plot

FLAGS = flags.FLAGS


def read_task_values(task_result: TaskResult) -> Tuple[str, str, float]:
    """Reads unitary and federated accuracy from results.json

    Args:
        fname (str): path to results.json file containing required fields

    Returns
        class, label, final_accuracy (str, str, float): e.g. ("VisionTask", "cpp01", 0.92)
    """
    return (
        task_result.get_class(),
        task_result.get_label(),
        task_result.get_final_accuracy(),
    )


def read_all_task_values(group_dir: str) -> List[Tuple[str, str, float]]:
    """
    Reads results directory for given group id and
    extracts values from results.json files

    :param filter_substring: has to be part of the dir name in results directory

    :returns: List of tuples (task_class, task_label, final_accuracy)
    """
    task_results = GroupResult(group_dir).get_results()
    # Read accuracies from each file and return list of values in tuples
    return [read_task_values(task_result) for task_result in task_results]


def group_values_by_class(
    values: List[Tuple[str, str, float]]
) -> Dict[str, List[Tuple[str, float]]]:
    # Get unique task classes
    task_classes = np.unique([v[0] for v in values])

    # Group values by task_class
    grouped_values: Dict[str, List[Tuple[str, float]]] = {
        task_class: [] for task_class in task_classes
    }

    for task_class in task_classes:
        filtered_values = [v for v in values if v[0] == task_class]
        for value in filtered_values:
            (_, label, acc) = value
            grouped_values[task_class].append((label, acc))

    return grouped_values


def prepare_aggregation_data(
    group_name: str
) -> Tuple[List[PlotValues], Optional[Tuple[XticksLocations, XticksLabels]]]:
    """Constructs and returns curves and xticks_args

    Args:
        group_name (str): group name for which to construct the curves

    Returns:
        (
            [("unitary", unitary_accuracies, indices),
            ("federated", federated_accuracies, indices)],
            (xticks_locations, xticks_labels))
        )
    """
    group_dir = os.path.join(FLAGS.results_dir, group_name)
    # List of tuples (benchmark_name, unitary_accuracy, federated_accuracy)
    values = read_all_task_values(group_dir=group_dir)
    values = sorted(values, key=lambda v: v[1], reverse=True)  # sort by

    assert values, "No values for group found"

    grouped_values = group_values_by_class(values)
    task_classes = [k for k in grouped_values]
    indices = list(range(1, len(grouped_values[task_classes[0]]) + 1))
    labels = [label for label, _ in grouped_values[task_classes[0]]]

    data: List[PlotValues] = []

    for task_class in grouped_values:
        task_class_values = [acc for _, acc in grouped_values[task_class]]
        plot_values = (task_class, task_class_values, indices)
        data.append(plot_values)

    return (data, (indices, labels))


def aggregate() -> str:
    """Plots IID and Non-IID dataset performance comparision

    :param data: List of tuples which represent (name, values, indices)
    :param fname: Filename of plot

    :returns: Absolut path to saved plot
    """
    group_name = FLAGS.group_name
    fname = f"plot_{group_name}.png"

    (data, xticks_args) = prepare_aggregation_data(group_name)

    assert len(data) == 2, "Expecting a list of two curves"

    fpath = plot(
        data,
        title="Max achieved accuracy for unitary and federated learning",
        xlabel="partitioning grade",
        ylabel="accuracy",
        fname=fname,
        save=True,
        show=False,
        ylim_max=1.0,
        xlim_max=12,
        xticks_args=xticks_args,
        legend_loc="upper right",
    )

    logging.info(f"Data plotted and saved in {fname}")

    return fpath


def app_run_aggregate():
    flags.mark_flag_as_required("group_name")
    app.run(main=lambda _: aggregate())

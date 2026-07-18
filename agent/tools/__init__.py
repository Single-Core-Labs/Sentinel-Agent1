"""
Sentinel AI tools for the agent
"""

from agent.tools.dataset_tools import (
    INSPECT_DATASET_TOOL_SPEC,
    inspect_dataset_handler,
)
from agent.tools.github_find_examples import (
    GITHUB_FIND_EXAMPLES_TOOL_SPEC,
    github_find_examples_handler,
)
from agent.tools.github_list_repos import (
    GITHUB_LIST_REPOS_TOOL_SPEC,
    github_list_repos_handler,
)
from agent.tools.github_read_file import (
    GITHUB_READ_FILE_TOOL_SPEC,
    github_read_file_handler,
)
from agent.tools.types import ToolResult
from agent.tools.git_tools import (
    GIT_COMMIT_TOOL_SPEC,
    GIT_DIFF_TOOL_SPEC,
    GIT_STATUS_TOOL_SPEC,
    _git_commit_handler,
    _git_diff_handler,
    _git_status_handler,
)
from agent.tools.terraform_tools import (
    TERRAFORM_APPLY_TOOL_SPEC,
    TERRAFORM_PLAN_TOOL_SPEC,
    TERRAFORM_STATE_TOOL_SPEC,
    _terraform_apply_handler,
    _terraform_plan_handler,
    _terraform_state_read_handler,
)
from agent.tools.web_search_tool import WEB_SEARCH_TOOL_SPEC, web_search_handler

__all__ = [
    "ToolResult",
    "GIT_STATUS_TOOL_SPEC",
    "GIT_DIFF_TOOL_SPEC",
    "GIT_COMMIT_TOOL_SPEC",
    "_git_status_handler",
    "_git_diff_handler",
    "_git_commit_handler",
    "TERRAFORM_PLAN_TOOL_SPEC",
    "TERRAFORM_STATE_TOOL_SPEC",
    "TERRAFORM_APPLY_TOOL_SPEC",
    "_terraform_plan_handler",
    "_terraform_state_read_handler",
    "_terraform_apply_handler",
    "GITHUB_FIND_EXAMPLES_TOOL_SPEC",
    "github_find_examples_handler",
    "GITHUB_LIST_REPOS_TOOL_SPEC",
    "github_list_repos_handler",
    "GITHUB_READ_FILE_TOOL_SPEC",
    "github_read_file_handler",
    "GITHUB_SEARCH_CODE_TOOL_SPEC",
    "github_search_code_handler",
    "INSPECT_DATASET_TOOL_SPEC",
    "inspect_dataset_handler",
    "WEB_SEARCH_TOOL_SPEC",
    "web_search_handler",
]

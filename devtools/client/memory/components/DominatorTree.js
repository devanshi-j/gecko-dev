/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this file,
 * You can obtain one at http://mozilla.org/MPL/2.0/. */

"use strict";

const {
  Component,
  createFactory,
} = require("resource://devtools/client/shared/vendor/react.mjs");
const dom = require("resource://devtools/client/shared/vendor/react-dom-factories.js");
const PropTypes = require("resource://devtools/client/shared/vendor/react-prop-types.mjs");
const { assert } = require("resource://devtools/shared/DevToolsUtils.js");
const {
  createParentMap,
} = require("resource://devtools/shared/heapsnapshot/CensusUtils.js");
const Tree = createFactory(
  require("resource://devtools/client/shared/components/VirtualizedTree.js")
);
const DominatorTreeItem = createFactory(
  require("resource://devtools/client/memory/components/DominatorTreeItem.js")
);
const { L10N } = require("resource://devtools/client/memory/utils.js");
const {
  TREE_ROW_HEIGHT,
  dominatorTreeState,
} = require("resource://devtools/client/memory/constants.js");
const {
  dominatorTreeModel,
} = require("resource://devtools/client/memory/models.js");
const DominatorTreeLazyChildren = require("resource://devtools/client/memory/dominator-tree-lazy-children.js");

const DOMINATOR_TREE_AUTO_EXPAND_DEPTH = 3;

/**
 * A throbber that represents a subtree in the dominator tree that is actively
 * being incrementally loaded and fetched from the `HeapAnalysesWorker`.
 */
class DominatorTreeSubtreeFetchingClass extends Component {
  static get propTypes() {
    return {
      depth: PropTypes.number.isRequired,
    };
  }

  shouldComponentUpdate(nextProps) {
    return this.props.depth !== nextProps.depth;
  }

  render() {
    const { depth } = this.props;

    return dom.div(
      {
        className: "heap-tree-item subtree-fetching",
      },
      dom.span({ className: "heap-tree-item-field heap-tree-item-bytes" }),
      dom.span({ className: "heap-tree-item-field heap-tree-item-bytes" }),
      dom.span({
        className: "heap-tree-item-field heap-tree-item-name devtools-throbber",
        style: { marginInlineStart: depth * TREE_ROW_HEIGHT },
      })
    );
  }
}

/**
 * A link to fetch and load more siblings in the dominator tree, when there are
 * already many loaded above.
 */
class DominatorTreeSiblingLinkClass extends Component {
  static get propTypes() {
    return {
      depth: PropTypes.number.isRequired,
      item: PropTypes.instanceOf(DominatorTreeLazyChildren).isRequired,
      onLoadMoreSiblings: PropTypes.func.isRequired,
    };
  }

  shouldComponentUpdate(nextProps) {
    return this.props.depth !== nextProps.depth;
  }

  render() {
    const { depth, item, onLoadMoreSiblings } = this.props;

    return dom.div(
      {
        className: `heap-tree-item more-children`,
      },
      dom.span({ className: "heap-tree-item-field heap-tree-item-bytes" }),
      dom.span({ className: "heap-tree-item-field heap-tree-item-bytes" }),
      dom.span(
        {
          className: "heap-tree-item-field heap-tree-item-name",
          style: { marginInlineStart: depth * TREE_ROW_HEIGHT },
        },
        dom.a(
          {
            className: "load-more-link",
            onClick: () => onLoadMoreSiblings(item),
          },
          L10N.getStr("tree-item.load-more")
        )
      )
    );
  }
}

class DominatorTree extends Component {
  static get propTypes() {
    return {
      dominatorTree: dominatorTreeModel.isRequired,
      onLoadMoreSiblings: PropTypes.func.isRequired,
      onViewSourceInDebugger: PropTypes.func.isRequired,
      onExpand: PropTypes.func.isRequired,
      onCollapse: PropTypes.func.isRequired,
      onFocus: PropTypes.func.isRequired,
    };
  }

  shouldComponentUpdate(nextProps) {
    // Safe to use referential equality here because all of our mutations on
    // dominator tree models use immutableUpdate in a persistent manner. The
    // exception to the rule are mutations of the expanded set, however we take
    // care that the dominatorTree model itself is still re-allocated when
    // mutations to the expanded set occur. Because of the re-allocations, we
    // can continue using referential equality here.
    return this.props.dominatorTree !== nextProps.dominatorTree;
  }

  render() {
    const { dominatorTree, onViewSourceInDebugger, onLoadMoreSiblings } =
      this.props;

    const parentMap = createParentMap(dominatorTree.root, node => node.nodeId);

    return Tree({
      key: "dominator-tree-tree",
      autoExpandDepth: DOMINATOR_TREE_AUTO_EXPAND_DEPTH,
      preventNavigationOnArrowRight: false,
      focused: dominatorTree.focused,
      getParent: node =>
        node instanceof DominatorTreeLazyChildren
          ? parentMap[node.parentNodeId()]
          : parentMap[node.nodeId],
      getChildren: node => {
        const children = node.children ? node.children.slice() : [];
        if (node.moreChildrenAvailable) {
          children.push(
            new DominatorTreeLazyChildren(node.nodeId, children.length)
          );
        }
        return children;
      },
      isExpanded: node => {
        return node instanceof DominatorTreeLazyChildren
          ? false
          : dominatorTree.expanded.has(node.nodeId);
      },
      onExpand: item => {
        if (item instanceof DominatorTreeLazyChildren) {
          return;
        }

        if (
          item.moreChildrenAvailable &&
          (!item.children || !item.children.length)
        ) {
          const startIndex = item.children ? item.children.length : 0;
          onLoadMoreSiblings(
            new DominatorTreeLazyChildren(item.nodeId, startIndex)
          );
        }

        this.props.onExpand(item);
      },
      onCollapse: item => {
        if (item instanceof DominatorTreeLazyChildren) {
          return;
        }

        this.props.onCollapse(item);
      },
      onFocus: item => {
        if (item instanceof DominatorTreeLazyChildren) {
          return;
        }

        this.props.onFocus(item);
      },
      renderItem: (item, depth, focused, arrow, expanded) => {
        if (item instanceof DominatorTreeLazyChildren) {
          if (item.isFirstChild()) {
            assert(
              dominatorTree.state === dominatorTreeState.INCREMENTAL_FETCHING,
              "If we are displaying a throbber for loading a subtree, " +
                "then we should be INCREMENTAL_FETCHING those children right now"
            );
            return DominatorTreeSubtreeFetching({
              key: item.key(),
              depth,
              focused,
            });
          }

          return DominatorTreeSiblingLink({
            key: item.key(),
            item,
            depth,
            onLoadMoreSiblings,
          });
        }

        return DominatorTreeItem({
          item,
          depth,
          arrow,
          expanded,
          getPercentSize: size =>
            (size / dominatorTree.root.retainedSize) * 100,
          onViewSourceInDebugger,
        });
      },
      getRoots: () => [dominatorTree.root],
      getKey: node =>
        node instanceof DominatorTreeLazyChildren ? node.key() : node.nodeId,
      itemHeight: TREE_ROW_HEIGHT,
    });
  }
}

const DominatorTreeSubtreeFetching = createFactory(
  DominatorTreeSubtreeFetchingClass
);
const DominatorTreeSiblingLink = createFactory(DominatorTreeSiblingLinkClass);

module.exports = DominatorTree;

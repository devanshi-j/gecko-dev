/* -*- js-indent-level: 2; indent-tabs-mode: nil -*- */
/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

import { AppConstants } from "resource://gre/modules/AppConstants.sys.mjs";

const lazy = {};

ChromeUtils.defineESModuleGetters(lazy, {
  WindowsRegistry: "resource://gre/modules/WindowsRegistry.sys.mjs",
  WindowsVersionInfo:
    "resource://gre/modules/components-utils/WindowsVersionInfo.sys.mjs",
});

export let OsEnvironment = {
  /**
   * This is a policy object used to override behavior for testing.
   */
  Policy: {
    getAllowedAppSources: () =>
      lazy.WindowsRegistry.readRegKey(
        Ci.nsIWindowsRegKey.ROOT_KEY_LOCAL_MACHINE,
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer",
        "AicEnabled"
      ),
    windowsVersionHasAppSourcesFeature: () => {
      let windowsVersion = parseFloat(Services.sysinfo.getProperty("version"));
      if (isNaN(windowsVersion)) {
        throw new Error("Unable to parse Windows version");
      }
      if (windowsVersion < 10) {
        return false;
      }

      // The App Sources feature was added in Windows 10, build 15063.
      const { buildNumber } = lazy.WindowsVersionInfo.get();
      return buildNumber >= 15063;
    },
  },

  reportAllowedAppSources() {
    if (AppConstants.platform != "win") {
      // This is currently a windows-only feature.
      return;
    }

    let haveAppSourcesFeature;
    try {
      haveAppSourcesFeature =
        OsEnvironment.Policy.windowsVersionHasAppSourcesFeature();
    } catch (ex) {
      console.error(ex);
      Glean.osEnvironment.allowedAppSources.set("Error");
      return;
    }
    if (!haveAppSourcesFeature) {
      Glean.osEnvironment.allowedAppSources.set("NoSuchFeature");
      return;
    }

    let allowedAppSources;
    try {
      allowedAppSources = OsEnvironment.Policy.getAllowedAppSources();
    } catch (ex) {
      console.error(ex);
      Glean.osEnvironment.allowedAppSources.set("Error");
      return;
    }
    if (allowedAppSources === undefined) {
      // A return value of undefined means that the registry value didn't
      // exist. Windows treats a missing registry entry the same as if the
      // value is "Anywhere". Make sure to differentiate a missing registry
      // entry from one containing an empty string, since it is unclear how
      // Windows would treat such a setting, but it may not be the same as
      // if the value is missing.
      allowedAppSources = "Anywhere";
    }

    const expectedValues = [
      "Anywhere",
      "Recommendations",
      "PreferStore",
      "StoreOnly",
    ];
    if (!expectedValues.includes(allowedAppSources)) {
      allowedAppSources = "Error";
    }

    Glean.osEnvironment.allowedAppSources.set(allowedAppSources);
  },
};
